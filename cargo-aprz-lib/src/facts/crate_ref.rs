use super::CrateSpec;
use core::fmt::{Display, Formatter, Result as FmtResult};
use core::str::FromStr;
use semver::Version;
use std::sync::Arc;

/// A crate identifier consisting of a name and optional version
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CrateRef {
    name: Arc<str>,
    version: Option<Arc<Version>>,
}

impl CrateRef {
    /// Create a new crate ID with name and optional version
    #[must_use]
    pub fn new(name: impl AsRef<str>, version: Option<Version>) -> Self {
        Self {
            name: Arc::from(name.as_ref()),
            version: version.map(Arc::new),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn version(&self) -> Option<&Version> {
        self.version.as_deref()
    }

    /// Get a clone of the name Arc
    #[must_use]
    pub fn name_arc(&self) -> Arc<str> {
        Arc::clone(&self.name)
    }

    /// Get a clone of the version Arc if present (cheap pointer clone, no Version allocation)
    #[must_use]
    pub fn version_arc(&self) -> Option<Arc<Version>> {
        self.version.as_ref().map(Arc::clone)
    }

    /// Convert to a `CrateSpec` by cloning Arc pointers (no allocation)
    #[must_use]
    pub fn to_spec(&self) -> Option<CrateSpec> {
        Some(CrateSpec::from_arcs(Arc::clone(&self.name), Arc::clone(self.version.as_ref()?)))
    }
}

impl FromStr for CrateRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        if let Some((name, version_str)) = s.split_once('@') {
            let version =
                Version::parse(version_str).map_err(|e| format!("invalid version '{version_str}' in crate specifier '{s}': {e}"))?;
            Ok(Self::new(name, Some(version)))
        } else {
            Ok(Self::new(s, None))
        }
    }
}

/* Replace the above once onho gets the right impl block for AppError
impl FromStr for CrateRef {
    type Err = ohno::AppError;

    fn from_str(s: &str) -> Result<Self> {
        if let Some((name, version_str)) = s.split_once('@') {
            let version =
                Version::parse(version_str).into_app_err_with(|| format!("invalid version '{version_str}' in crate specifier '{s}'"))?;
            Ok(Self::new(name, Some(version)))
        } else {
            Ok(Self::new(s, None))
        }
    }
}
*/

impl Display for CrateRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}", self.name())?;
        if let Some(version) = self.version() {
            write!(f, "@{version}")?;
        }
        Ok(())
    }
}
