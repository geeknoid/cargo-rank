use super::Host;
use super::common::{Common, CommonArgs};
use crate::Result;
use crate::facts::CrateRef;
use cargo_metadata::{CargoOpt, Dependency, DependencyKind, Node, Package, PackageId};
use clap::{Parser, ValueEnum};
use ohno::{IntoAppError, bail};
use serde::{Deserialize, Serialize};
use crate::{HashMap, HashSet};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum, Deserialize, Serialize, Display, EnumString)]
#[value(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DependencyType {
    /// Regular production dependencies
    Standard,

    /// Development-only dependencies
    Dev,

    /// Build-only dependencies
    Build,
}

#[derive(Parser, Debug)]
pub struct DepsArgs {
    /// Comma-separated list of dependency types to appraise
    #[arg(
        long = "dependency-types",
        value_delimiter = ',',
        value_name = "TYPES",
        default_value = "standard,dev,build"
    )]
    pub dependency_types: Option<Vec<DependencyType>>,

    /// Space or comma separated list of features to activate
    #[arg(short = 'F', long, value_name = "FEATURES", help_heading = "Feature Selection")]
    pub features: Vec<String>,

    /// Activate all available features
    #[arg(long, help_heading = "Feature Selection")]
    pub all_features: bool,

    /// Do not activate the `default` feature
    #[arg(long, help_heading = "Feature Selection")]
    pub no_default_features: bool,

    /// Process only the specified package
    #[arg(short = 'p', long, value_name = "SPEC", help_heading = "Package Selection")]
    pub package: Vec<String>,

    /// Process all packages in the workspace
    #[arg(long, help_heading = "Package Selection")]
    pub workspace: bool,

    #[command(flatten)]
    pub common: CommonArgs,
}

pub async fn process_dependencies<H: Host>(host: &mut H, args: &DepsArgs) -> Result<()> {
    let mut common = Common::new(host, &args.common).await?;

    // Configure features on the metadata command based on command-line options
    if args.all_features {
        _ = common.metadata_cmd.features(CargoOpt::AllFeatures);
    } else {
        if args.no_default_features {
            _ = common.metadata_cmd.features(CargoOpt::NoDefaultFeatures);
        }

        if !args.features.is_empty() {
            _ = common.metadata_cmd.features(CargoOpt::SomeFeatures(args.features.clone()));
        }
    }

    let metadata = common.metadata_cmd.exec().into_app_err("retrieving workspace metadata")?;
    let all_packages: HashMap<_, _> = metadata.packages.iter().map(|p| (&p.id, p)).collect();
    let resolve_index: HashMap<&PackageId, &Node> = metadata
        .resolve
        .as_ref()
        .map_or_else(HashMap::default, |r| r.nodes.iter().map(|n| (&n.id, n)).collect());

    // Validate package names if specified
    if !args.package.is_empty() {
        for pkg_name in &args.package {
            let found = metadata
                .workspace_members
                .iter()
                .filter_map(|id| all_packages.get(id).map(|p| &p.name))
                .any(|name| name == pkg_name);
            if !found {
                bail!("package '{pkg_name}' not found in workspace");
            }
        }
    }

    if !args.package.is_empty() {
        process_packages(
            args,
            &mut common,
            &all_packages,
            &resolve_index,
            metadata
                .workspace_members
                .iter()
                .filter_map(|id| all_packages.get(id).copied())
                .filter(|p| args.package.contains(&p.name)),
        )
        .await
    } else if args.workspace {
        process_packages(
            args,
            &mut common,
            &all_packages,
            &resolve_index,
            metadata.workspace_members.iter().filter_map(|id| all_packages.get(id).copied()),
        )
        .await
    } else if let Some(root) = metadata.root_package() {
        process_packages(args, &mut common, &all_packages, &resolve_index, core::iter::once(root)).await
    } else {
        // Virtual workspace, default to all members
        process_packages(
            args,
            &mut common,
            &all_packages,
            &resolve_index,
            metadata.workspace_members.iter().filter_map(|id| all_packages.get(id).copied()),
        )
        .await
    }
}

async fn process_packages<'a, H: Host>(
    args: &DepsArgs,
    common: &mut Common<'_, H>,
    all_packages: &HashMap<&'a PackageId, &'a Package>,
    resolve_index: &HashMap<&'a PackageId, &'a Node>,
    target_packages: impl Iterator<Item = &'a Package>,
) -> Result<()> {
    let should_process = |dep_type: &DependencyType| {
        args.dependency_types
            .as_ref()
            .is_none_or(|d| d.is_empty() || d.contains(dep_type))
    };

    // Collect all (CrateId, dependency_type) pairs, preserving duplicates
    let mut crate_dep_pairs: Vec<(CrateRef, DependencyType)> = Vec::new();

    let active_dep_types: Vec<_> = [DependencyType::Standard, DependencyType::Dev, DependencyType::Build]
        .into_iter()
        .filter(|dt| should_process(dt))
        .collect();

    for package in target_packages {
        for &dep_type in &active_dep_types {
            crate_dep_pairs.extend(build_transitive_deps(
                all_packages,
                resolve_index,
                &package.id,
                dep_type,
            ));
        }
    }

    // Fetch facts for each crate (no suggestions for deps command)
    let crate_refs: Vec<CrateRef> = crate_dep_pairs.into_iter().map(|(crate_ref, _)| crate_ref).collect();
    let facts = common
        .process_crates(&crate_refs, false)
        .await?;

    // Report the facts
    common.report(facts.into_iter())
}

/// Expand a set of features transitively using the package's feature declarations.
///
/// For each enabled feature, follows feature-to-feature activations (entries without
/// `:` or `/` separators) to compute the full set of active features.
fn expand_features(pkg: &Package, initial_features: &HashSet<String>) -> HashSet<String> {
    let mut expanded = initial_features.clone();
    let mut queue: Vec<String> = initial_features.iter().cloned().collect();

    while let Some(feature) = queue.pop() {
        if let Some(activations) = pkg.features.get(&feature) {
            for activation in activations {
                if activation.contains(':') || activation.contains('/') {
                    continue;
                }
                if expanded.insert(activation.clone()) {
                    queue.push(activation.clone());
                }
            }
        }
    }

    expanded
}

/// Find the dependency declaration in a package that matches the given library name and kind.
///
/// Falls back to matching any kind if no exact kind match is found.
fn find_dep_declaration<'a>(
    pkg: &'a Package,
    dep_lib_name: &str,
    kind: DependencyKind,
) -> Option<&'a Dependency> {
    pkg.dependencies
        .iter()
        .find(|d| d.rename.as_deref().unwrap_or(&d.name) == dep_lib_name && d.kind == kind)
        .or_else(|| {
            pkg.dependencies
                .iter()
                .find(|d| d.rename.as_deref().unwrap_or(&d.name) == dep_lib_name)
        })
}

/// Check whether an optional dependency is activated by the given set of expanded features.
fn is_optional_dep_active(expanded_features: &HashSet<String>, pkg: &Package, dep_lib_name: &str) -> bool {
    // Implicit feature: the dep name itself appears as an enabled feature
    if expanded_features.contains(dep_lib_name) {
        return true;
    }

    let slash_prefix = format!("{dep_lib_name}/");

    for feature_name in expanded_features {
        if let Some(activations) = pkg.features.get(feature_name) {
            for activation in activations {
                // dep:name syntax (edition 2021+)
                if activation.strip_prefix("dep:") == Some(dep_lib_name) {
                    return true;
                }
                // dep_name/feature syntax activates the optional dep (but dep_name?/feature does not)
                if activation.starts_with(slash_prefix.as_str()) {
                    return true;
                }
                // Pre-2021: listing an optional dep name directly in a feature activates it
                if activation == dep_lib_name && !activation.contains(':') && !activation.contains('/') {
                    return true;
                }
            }
        }
    }

    false
}

/// Compute the features to activate on a dependency based on the parent's declaration and features.
fn compute_dep_features(
    parent_pkg: &Package,
    dep_decl: &Dependency,
    parent_expanded_features: &HashSet<String>,
) -> HashSet<String> {
    let mut features = HashSet::default();

    if dep_decl.uses_default_features {
        _ = features.insert("default".to_string());
    }

    for f in &dep_decl.features {
        _ = features.insert(f.clone());
    }

    // Propagate features from parent feature declarations (dep/feature and dep?/feature syntax)
    let dep_lib_name = dep_decl.rename.as_deref().unwrap_or(&dep_decl.name);
    let prefix = format!("{dep_lib_name}/");
    let weak_prefix = format!("{dep_lib_name}?/");

    for feature_name in parent_expanded_features {
        if let Some(activations) = parent_pkg.features.get(feature_name) {
            for activation in activations {
                if let Some(dep_feature) = activation.strip_prefix(prefix.as_str()) {
                    _ = features.insert(dep_feature.to_string());
                } else if let Some(dep_feature) = activation.strip_prefix(weak_prefix.as_str()) {
                    _ = features.insert(dep_feature.to_string());
                }
            }
        }
    }

    features
}

/// Build the transitive closure of dependencies starting from a target package.
///
/// Uses the resolved dependency graph from `cargo metadata` to walk exact `PackageId`s,
/// avoiding ambiguity when multiple versions of the same crate exist (e.g., syn 1.x and 2.x).
///
/// Feature-aware: only follows optional dependencies whose activating features are enabled,
/// and propagates the correct feature set to each dependency based on the parent's declaration
/// rather than the unified `Node.features` from cargo metadata. This avoids false positives
/// caused by workspace feature unification.
///
/// Dev/build dependencies only apply at the first hop; their transitive deps are Normal.
fn build_transitive_deps<'a>(
    all_packages: &HashMap<&'a PackageId, &'a Package>,
    resolve_index: &HashMap<&'a PackageId, &'a Node>,
    target_package_id: &PackageId,
    dependency_type: DependencyType,
) -> HashSet<(CrateRef, DependencyType)> {
    let initial_kind = match dependency_type {
        DependencyType::Standard => DependencyKind::Normal,
        DependencyType::Dev => DependencyKind::Development,
        DependencyType::Build => DependencyKind::Build,
    };

    let mut result = HashSet::default();
    let mut visited_features: HashMap<&PackageId, HashSet<String>> = HashMap::default();
    let mut queue: Vec<(&PackageId, HashSet<String>)> = Vec::new();

    // Seed the queue with the target package's direct deps of the requested kind
    if let Some(target_pkg) = all_packages.get(target_package_id)
        && let Some(node) = resolve_index.get(target_package_id)
    {
        let root_features: HashSet<String> = node.features.iter().map(ToString::to_string).collect();
        let expanded_root = expand_features(target_pkg, &root_features);

        for node_dep in &node.deps {
            if node_dep.dep_kinds.iter().any(|dk| dk.kind == initial_kind) {
                if let Some(dep_decl) = find_dep_declaration(target_pkg, &node_dep.name, initial_kind) {
                    if dep_decl.optional
                        && !is_optional_dep_active(&expanded_root, target_pkg, &node_dep.name)
                    {
                        continue;
                    }
                    let features = compute_dep_features(target_pkg, dep_decl, &expanded_root);
                    queue.push((&node_dep.pkg, features));
                } else {
                    queue.push((&node_dep.pkg, HashSet::default()));
                }
            }
        }
    }

    while let Some((pkg_id, activated_features)) = queue.pop() {
        // Only re-process if we have new features to consider
        let was_seen = visited_features.contains_key(pkg_id);
        let entry = visited_features.entry(pkg_id).or_default();
        let prev_len = entry.len();
        entry.extend(activated_features);
        if was_seen && entry.len() == prev_len {
            continue;
        }
        let all_features = entry.clone();

        if let Some(pkg) = all_packages.get(pkg_id) {
            _ = result.insert((CrateRef::new(&pkg.name, Some(pkg.version.clone())), dependency_type));

            let expanded = expand_features(pkg, &all_features);

            // Follow Normal edges for transitive deps, filtered by feature activation
            if let Some(node) = resolve_index.get(pkg_id) {
                for node_dep in &node.deps {
                    if node_dep.dep_kinds.iter().any(|dk| dk.kind == DependencyKind::Normal) {
                        if let Some(dep_decl) =
                            find_dep_declaration(pkg, &node_dep.name, DependencyKind::Normal)
                        {
                            if dep_decl.optional
                                && !is_optional_dep_active(&expanded, pkg, &node_dep.name)
                            {
                                continue;
                            }
                            let dep_features = compute_dep_features(pkg, dep_decl, &expanded);
                            queue.push((&node_dep.pkg, dep_features));
                        } else {
                            queue.push((&node_dep.pkg, HashSet::default()));
                        }
                    }
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_package(json: &str) -> Package {
        serde_json::from_str(json).expect("valid Package JSON")
    }

    fn make_dep(json: &str) -> Dependency {
        serde_json::from_str(json).expect("valid Dependency JSON")
    }

    const MINIMAL_PKG: &str = r#"{
        "name": "test-pkg",
        "version": "0.1.0",
        "id": "test-pkg 0.1.0 (path+file:///test)",
        "source": null,
        "dependencies": [],
        "targets": [],
        "features": {},
        "manifest_path": "/test/Cargo.toml",
        "categories": [],
        "keywords": [],
        "edition": "2021",
        "metadata": null
    }"#;

    #[test]
    fn expand_features_empty() {
        let pkg = make_package(MINIMAL_PKG);
        let result = expand_features(&pkg, &HashSet::default());
        assert!(result.is_empty());
    }

    #[test]
    fn expand_features_transitive() {
        let pkg = make_package(r#"{
            "name": "test-pkg", "version": "0.1.0",
            "id": "test-pkg 0.1.0 (path+file:///test)", "source": null,
            "dependencies": [], "targets": [],
            "features": {
                "default": ["a"],
                "a": ["b"],
                "b": []
            },
            "manifest_path": "/test/Cargo.toml", "categories": [], "keywords": [],
            "edition": "2021", "metadata": null
        }"#);

        let initial: HashSet<String> = core::iter::once("default".to_string()).collect();
        let expanded = expand_features(&pkg, &initial);
        assert!(expanded.contains("default"));
        assert!(expanded.contains("a"));
        assert!(expanded.contains("b"));
    }

    #[test]
    fn expand_features_skips_dep_syntax() {
        let pkg = make_package(r#"{
            "name": "test-pkg", "version": "0.1.0",
            "id": "test-pkg 0.1.0 (path+file:///test)", "source": null,
            "dependencies": [], "targets": [],
            "features": {
                "extra": ["dep:serde", "itoa/serde", "b"],
                "b": []
            },
            "manifest_path": "/test/Cargo.toml", "categories": [], "keywords": [],
            "edition": "2021", "metadata": null
        }"#);

        let initial: HashSet<String> = core::iter::once("extra".to_string()).collect();
        let expanded = expand_features(&pkg, &initial);
        assert!(expanded.contains("extra"));
        assert!(expanded.contains("b"));
        // dep:serde and itoa/serde are dependency syntax, not features
        assert!(!expanded.contains("dep:serde"));
        assert!(!expanded.contains("itoa/serde"));
        assert_eq!(expanded.len(), 2);
    }

    #[test]
    fn find_dep_declaration_by_name_and_kind() {
        let pkg = make_package(r#"{
            "name": "test-pkg", "version": "0.1.0",
            "id": "test-pkg 0.1.0 (path+file:///test)", "source": null,
            "dependencies": [
                {"name": "serde", "req": "^1", "kind": null, "optional": true, "uses_default_features": true, "features": [], "target": null, "rename": null},
                {"name": "serde", "req": "^1", "kind": "dev", "optional": false, "uses_default_features": true, "features": ["derive"], "target": null, "rename": null}
            ],
            "targets": [], "features": {},
            "manifest_path": "/test/Cargo.toml", "categories": [], "keywords": [],
            "edition": "2021", "metadata": null
        }"#);

        let normal = find_dep_declaration(&pkg, "serde", DependencyKind::Normal);
        assert!(normal.is_some());
        assert!(normal.unwrap().optional);

        let dev = find_dep_declaration(&pkg, "serde", DependencyKind::Development);
        assert!(dev.is_some());
        assert!(!dev.unwrap().optional);
    }

    #[test]
    fn find_dep_declaration_with_rename() {
        let pkg = make_package(r#"{
            "name": "test-pkg", "version": "0.1.0",
            "id": "test-pkg 0.1.0 (path+file:///test)", "source": null,
            "dependencies": [
                {"name": "serde", "req": "^1", "kind": null, "optional": false, "uses_default_features": true, "features": [], "target": null, "rename": "my_serde"}
            ],
            "targets": [], "features": {},
            "manifest_path": "/test/Cargo.toml", "categories": [], "keywords": [],
            "edition": "2021", "metadata": null
        }"#);

        // Match by renamed name
        let found = find_dep_declaration(&pkg, "my_serde", DependencyKind::Normal);
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "serde");

        // Original name should NOT match
        let not_found = find_dep_declaration(&pkg, "serde", DependencyKind::Normal);
        assert!(not_found.is_none());
    }

    #[test]
    fn is_optional_dep_active_dep_colon_syntax() {
        let pkg = make_package(r#"{
            "name": "test-pkg", "version": "0.1.0",
            "id": "test-pkg 0.1.0 (path+file:///test)", "source": null,
            "dependencies": [], "targets": [],
            "features": {
                "extra": ["dep:once_cell"]
            },
            "manifest_path": "/test/Cargo.toml", "categories": [], "keywords": [],
            "edition": "2021", "metadata": null
        }"#);

        let features: HashSet<String> = core::iter::once("extra".to_string()).collect();
        assert!(is_optional_dep_active(&features, &pkg, "once_cell"));
        assert!(!is_optional_dep_active(&features, &pkg, "serde"));
    }

    #[test]
    fn is_optional_dep_active_implicit_feature() {
        let pkg = make_package(r#"{
            "name": "test-pkg", "version": "0.1.0",
            "id": "test-pkg 0.1.0 (path+file:///test)", "source": null,
            "dependencies": [], "targets": [],
            "features": {},
            "manifest_path": "/test/Cargo.toml", "categories": [], "keywords": [],
            "edition": "2021", "metadata": null
        }"#);

        // Pre-2021 style: feature name matches dep name
        let features: HashSet<String> = core::iter::once("serde".to_string()).collect();
        assert!(is_optional_dep_active(&features, &pkg, "serde"));
        assert!(!is_optional_dep_active(&features, &pkg, "other"));
    }

    #[test]
    fn is_optional_dep_active_not_active() {
        let pkg = make_package(r#"{
            "name": "test-pkg", "version": "0.1.0",
            "id": "test-pkg 0.1.0 (path+file:///test)", "source": null,
            "dependencies": [], "targets": [],
            "features": {
                "extra": ["dep:once_cell"]
            },
            "manifest_path": "/test/Cargo.toml", "categories": [], "keywords": [],
            "edition": "2021", "metadata": null
        }"#);

        // Without the "extra" feature, once_cell should NOT be active
        let empty: HashSet<String> = HashSet::default();
        assert!(!is_optional_dep_active(&empty, &pkg, "once_cell"));
    }

    #[test]
    fn is_optional_dep_active_slash_syntax() {
        let pkg = make_package(r#"{
            "name": "test-pkg", "version": "0.1.0",
            "id": "test-pkg 0.1.0 (path+file:///test)", "source": null,
            "dependencies": [], "targets": [],
            "features": {
                "extra": ["itoa/serde"],
                "weak": ["itoa?/serde"]
            },
            "manifest_path": "/test/Cargo.toml", "categories": [], "keywords": [],
            "edition": "2021", "metadata": null
        }"#);

        // dep/feature syntax activates the optional dep
        let features: HashSet<String> = core::iter::once("extra".to_string()).collect();
        assert!(is_optional_dep_active(&features, &pkg, "itoa"));

        // dep?/feature (weak) syntax does NOT activate the optional dep
        let weak_features: HashSet<String> = core::iter::once("weak".to_string()).collect();
        assert!(!is_optional_dep_active(&weak_features, &pkg, "itoa"));
    }

    #[test]
    fn compute_dep_features_basic(){
        let pkg = make_package(MINIMAL_PKG);
        let dep = make_dep(r#"{
            "name": "serde", "req": "^1", "kind": null,
            "optional": false, "uses_default_features": true,
            "features": ["derive"], "target": null, "rename": null
        }"#);

        let parent_features = HashSet::default();
        let features = compute_dep_features(&pkg, &dep, &parent_features);
        assert!(features.contains("default"));
        assert!(features.contains("derive"));
        assert_eq!(features.len(), 2);
    }

    #[test]
    fn compute_dep_features_no_default() {
        let pkg = make_package(MINIMAL_PKG);
        let dep = make_dep(r#"{
            "name": "serde", "req": "^1", "kind": null,
            "optional": false, "uses_default_features": false,
            "features": ["derive"], "target": null, "rename": null
        }"#);

        let parent_features = HashSet::default();
        let features = compute_dep_features(&pkg, &dep, &parent_features);
        assert!(!features.contains("default"));
        assert!(features.contains("derive"));
        assert_eq!(features.len(), 1);
    }

    #[test]
    fn compute_dep_features_propagates_from_parent() {
        let pkg = make_package(r#"{
            "name": "test-pkg", "version": "0.1.0",
            "id": "test-pkg 0.1.0 (path+file:///test)", "source": null,
            "dependencies": [], "targets": [],
            "features": {
                "serde": ["dep:serde", "itoa/serde"],
                "extra": ["itoa?/extra_feature"]
            },
            "manifest_path": "/test/Cargo.toml", "categories": [], "keywords": [],
            "edition": "2021", "metadata": null
        }"#);
        let dep = make_dep(r#"{
            "name": "itoa", "req": "^1", "kind": null,
            "optional": false, "uses_default_features": false,
            "features": [], "target": null, "rename": null
        }"#);

        let parent_features: HashSet<String> = ["serde".to_string(), "extra".to_string()].into_iter().collect();
        let features = compute_dep_features(&pkg, &dep, &parent_features);
        // itoa/serde from "serde" feature, itoa?/extra_feature from "extra" feature
        assert!(features.contains("serde"));
        assert!(features.contains("extra_feature"));
    }
}
