use super::crate_overall_data::CrateOverallData;
use super::crate_version_data::CrateVersionData;
use serde::{Deserialize, Serialize};

/// Combined crate information including both version-specific and overall data.
///
/// This struct bundles together all crate-related information from the crates.io database,
/// including both the requested version's specific data and the crate's overall metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CratesData {
    /// Version-specific crate information.
    ///
    /// Contains metadata specific to the requested version, such as description,
    /// license, features, and version-specific download counts.
    pub version_data: CrateVersionData,

    /// Version-independent crate information.
    ///
    /// Contains metadata that applies to the crate as a whole, such as the crate name,
    /// repository URL, owners, categories, and total download counts across all versions.
    pub overall_data: CrateOverallData,
}

impl CratesData {
    /// Creates a new `CratesData` instance.
    #[must_use]
    pub const fn new(version_data: CrateVersionData, overall_data: CrateOverallData) -> Self {
        Self {
            version_data,
            overall_data,
        }
    }
}
