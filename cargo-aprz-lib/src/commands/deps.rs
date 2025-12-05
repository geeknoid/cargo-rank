use super::Host;
use super::common::{Common, CommonArgs};
use crate::Result;
use crate::facts::CrateRef;
use cargo_metadata::{CargoOpt, DependencyKind, Package, PackageId};
use clap::{Parser, ValueEnum};
use ohno::{IntoAppError, bail};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum, Deserialize, Serialize, Display, EnumString)]
#[value(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DependencyType {
    Standard,
    Dev,
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

    let metadata = common.metadata_cmd.exec().into_app_err("unable to retrieve workspace metadata")?;
    let all_packages: HashMap<_, _> = metadata.packages.iter().map(|p| (&p.id, p)).collect();

    // Validate package names if specified
    if !args.package.is_empty() {
        let workspace_packages: Vec<_> = metadata
            .workspace_members
            .iter()
            .filter_map(|id| all_packages.get(id).map(|p| &p.name))
            .collect();

        for pkg_name in &args.package {
            if !workspace_packages.iter().any(|&name| name == pkg_name) {
                bail!("package '{pkg_name}' not found in workspace");
            }
        }
    }

    if !args.package.is_empty() {
        process_packages(
            args,
            &mut common,
            &all_packages,
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
            metadata.workspace_members.iter().filter_map(|id| all_packages.get(id).copied()),
        )
        .await
    } else if let Some(root) = metadata.root_package() {
        process_packages(args, &mut common, &all_packages, core::iter::once(root)).await
    } else {
        // Virtual workspace, default to all members
        process_packages(
            args,
            &mut common,
            &all_packages,
            metadata.workspace_members.iter().filter_map(|id| all_packages.get(id).copied()),
        )
        .await
    }
}

async fn process_packages<'a, H: Host>(
    args: &DepsArgs,
    common: &mut Common<'_, H>,
    all_packages: &HashMap<&'a PackageId, &'a Package>,
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
                package
                    .dependencies
                    .iter()
                    .filter(|d| d.kind == DependencyKind::Normal)
                    .map(|d| d.name.as_str()),
                DependencyType::Standard,
            ));
        }

        if should_process_dev {
            crate_dep_pairs.extend(build_transitive_deps(
                all_packages,
                package
                    .dependencies
                    .iter()
                    .filter(|d| d.kind == DependencyKind::Development)
                    .map(|d| d.name.as_str()),
                DependencyType::Dev,
            ));
        }

        if should_process_build {
            crate_dep_pairs.extend(build_transitive_deps(
                all_packages,
                package
                    .dependencies
                    .iter()
                    .filter(|d| d.kind == DependencyKind::Build)
                    .map(|d| d.name.as_str()),
                DependencyType::Build,
            ));
        }
    }

    // Fetch facts for each crate (no suggestions for deps command)
    let facts = common
        .process_crates(crate_dep_pairs.iter().map(|(crate_ref, _)| crate_ref.clone()), false)
        .await?;

    // Report the facts
    common.report(facts.into_iter())
}

/// Build the transitive closure of dependencies starting from `initial_deps`
fn build_transitive_deps<'a>(
    all_packages: &HashMap<&'a PackageId, &'a Package>,
    initial_deps: impl IntoIterator<Item = &'a str>,
    dependency_type: DependencyType,
) -> HashSet<(CrateRef, DependencyType)> {
    // Convert DependencyType to DependencyKind for filtering
    let dependency_kind = match dependency_type {
        DependencyType::Standard => DependencyKind::Normal,
        DependencyType::Dev => DependencyKind::Development,
        DependencyType::Build => DependencyKind::Build,
    };

    let mut result = HashSet::new();
    let mut queue: Vec<&str> = initial_deps.into_iter().collect();
    let mut visited_names = HashSet::new();

    while let Some(dep_name) = queue.pop() {
        if !visited_names.insert(dep_name) {
            continue; // Already processed
        }

        // Find the package for this dependency
        if let Some(pkg) = all_packages.values().find(|p| p.name == dep_name) {
            _ = result.insert((CrateRef::new(&pkg.name, Some(pkg.version.clone())), dependency_type));

            for dep in &pkg.dependencies {
                if dep.kind == dependency_kind {
                    queue.push(dep.name.as_str());
                }
            }
        }
    }

    result
}
