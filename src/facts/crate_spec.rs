use crate::facts::repo_spec::RepoSpec;
use core::fmt::{Display, Formatter, Result as FmtResult};
use semver::Version;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrateSpec {
    name: Arc<str>,
    version: Arc<Version>,
    repo_spec: Option<RepoSpec>,
}

impl CrateSpec {
    #[must_use]
    pub const fn from_arcs(name: Arc<str>, version: Arc<Version>) -> Self {
        Self {
            name,
            version,
            repo_spec: None,
        }
    }

    #[must_use]
    pub const fn from_arcs_with_repo(name: Arc<str>, version: Arc<Version>, repo_spec: RepoSpec) -> Self {
        Self {
            name,
            version,
            repo_spec: Some(repo_spec),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn version(&self) -> &Version {
        &self.version
    }

    #[must_use]
    pub const fn repo(&self) -> Option<&RepoSpec> {
        self.repo_spec.as_ref()
    }
}

/// Group crate by their repos
#[must_use]
pub fn by_repo(specs: impl IntoIterator<Item = CrateSpec>) -> HashMap<RepoSpec, Vec<CrateSpec>> {
    let mut repo_crates: HashMap<RepoSpec, Vec<CrateSpec>> = HashMap::new();
    for crate_spec in specs {
        if let Some(repo_spec) = &crate_spec.repo_spec {
            repo_crates.entry(repo_spec.clone()).or_default().push(crate_spec);
        }
    }

    repo_crates
}

impl Display for CrateSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}@{}", self.name(), self.version())?;
        Ok(())
    }
}
