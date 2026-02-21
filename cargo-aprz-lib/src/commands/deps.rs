use super::Host;
use super::common::{Common, CommonArgs};
use crate::Result;
use crate::facts::CrateRef;
use cargo_metadata::{CargoOpt, DependencyKind, Node, Package, PackageId};
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
    let should_process_std = args
        .dependency_types
        .as_ref()
        .is_none_or(|d| d.is_empty() || d.contains(&DependencyType::Standard));
    let should_process_dev = args
        .dependency_types
        .as_ref()
        .is_none_or(|d| d.is_empty() || d.contains(&DependencyType::Dev));
    let should_process_build = args
        .dependency_types
        .as_ref()
        .is_none_or(|d| d.is_empty() || d.contains(&DependencyType::Build));

    // Collect all (CrateId, dependency_type) pairs, preserving duplicates
    let mut crate_dep_pairs: Vec<(CrateRef, DependencyType)> = Vec::new();

    for package in target_packages {
        if should_process_std {
            crate_dep_pairs.extend(build_transitive_deps(
                all_packages,
                resolve_index,
                &package.id,
                DependencyType::Standard,
            ));
        }

        if should_process_dev {
            crate_dep_pairs.extend(build_transitive_deps(all_packages, resolve_index, &package.id, DependencyType::Dev));
        }

        if should_process_build {
            crate_dep_pairs.extend(build_transitive_deps(
                all_packages,
                resolve_index,
                &package.id,
                DependencyType::Build,
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

/// Build the transitive closure of dependencies starting from a target package.
///
/// Uses the resolved dependency graph from `cargo metadata` to walk exact `PackageId`s,
/// avoiding ambiguity when multiple versions of the same crate exist (e.g., syn 1.x and 2.x).
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
    let mut visited: HashSet<&PackageId> = HashSet::default();
    let mut queue: Vec<&PackageId> = Vec::new();

    // Seed the queue with the target package's direct deps of the requested kind
    if let Some(node) = resolve_index.get(target_package_id) {
        for dep in &node.deps {
            if dep.dep_kinds.iter().any(|dk| dk.kind == initial_kind) {
                queue.push(&dep.pkg);
            }
        }
    }

    while let Some(pkg_id) = queue.pop() {
        if !visited.insert(pkg_id) {
            continue;
        }

        if let Some(pkg) = all_packages.get(pkg_id) {
            _ = result.insert((CrateRef::new(&pkg.name, Some(pkg.version.clone())), dependency_type));

            // Follow Normal edges for all transitive deps (initial deps already
            // had their kind applied when seeding the queue above)
            if let Some(node) = resolve_index.get(pkg_id) {
                for dep in &node.deps {
                    if dep.dep_kinds.iter().any(|dk| dk.kind == DependencyKind::Normal) {
                        queue.push(&dep.pkg);
                    }
                }
            }
        }
    }

    result
}
