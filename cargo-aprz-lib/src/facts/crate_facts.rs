use super::ProviderResult;
use super::advisories::AdvisoryData;
use super::codebase::CodebaseData;
use super::coverage::CoverageData;
use super::crate_spec::CrateSpec;
use super::crates::CratesData;
use super::docs::DocsData;
use super::hosting::HostingData;

/// Comprehensive facts about a crate collected from various providers
#[derive(Debug)]
pub struct CrateFacts {
    pub crate_spec: CrateSpec,
    pub crates_data: ProviderResult<CratesData>,
    pub hosting_data: ProviderResult<HostingData>,
    pub advisory_data: ProviderResult<AdvisoryData>,
    pub codebase_data: ProviderResult<CodebaseData>,
    pub coverage_data: ProviderResult<CoverageData>,
    pub docs_data: ProviderResult<DocsData>,
}
