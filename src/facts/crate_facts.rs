use crate::facts::ProviderResult;
use crate::facts::advisories::AdvisoryData;
use crate::facts::codebase::CodebaseData;
use crate::facts::coverage::CoverageData;
use crate::facts::crates::{CrateOverallData, CrateVersionData};
use crate::facts::docs::DocsData;
use crate::facts::hosting::HostingData;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Comprehensive facts about a crate collected from various sources
#[derive(Debug, Deserialize, Serialize)]
pub struct CrateFacts {
    pub collected_at: DateTime<Utc>,
    pub crate_version_data: ProviderResult<CrateVersionData>,
    pub crate_overall_data: ProviderResult<CrateOverallData>,
    pub hosting_data: ProviderResult<HostingData>,
    pub advisory_data: ProviderResult<AdvisoryData>,
    pub codebase_data: ProviderResult<CodebaseData>,
    pub coverage_data: ProviderResult<CoverageData>,
    pub docs_data: ProviderResult<DocsData>,
}

impl CrateFacts {
    /// Returns true if all provider results are Found (no errors or missing data)
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.crate_version_data.is_found()
            && self.crate_overall_data.is_found()
            && self.hosting_data.is_found()
            && self.advisory_data.is_found()
            && self.codebase_data.is_found()
            && self.coverage_data.is_found()
            && self.docs_data.is_found()
    }
}
