use crate::metrics::metric_category::MetricCategory;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumIter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Display, EnumIter)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Metric {
    License,
    Age,
    MinVersion,
    ReleaseCount,

    OverallDownloadCount,
    OneMonthDownloadCount,

    OverallOwnerCount,
    UserOwnerCount,
    TeamOwnerCount,

    DependentCount,
    DirectDependencyCount,
    TransitiveDependencyCount,

    DocCoveragePercentage,
    BrokenDocLinkCount,
    CodeCoveragePercentage,
    FullySafeCode,
    ExampleCount,

    RepoStarCount,
    RepoForkCount,
    RepoSubscriberCount,
    RepoContributorCount,
    CommitActivity,

    OpenIssueCount,
    ClosedIssueCount,
    IssueResponsiveness,

    OpenPullRequestCount,
    ClosedPullRequestCount,
    PullRequestResponsiveness,

    VulnerabilityCount,
    LowVulnerabilityCount,
    MediumVulnerabilityCount,
    HighVulnerabilityCount,
    CriticalVulnerabilityCount,
    WarningCount,
    NoticeWarningCount,
    UnmaintainedWarningCount,
    UnsoundWarningCount,
    YankedWarningCount,

    HistoricalVulnerabilityCount,
    HistoricalLowVulnerabilityCount,
    HistoricalMediumVulnerabilityCount,
    HistoricalHighVulnerabilityCount,
    HistoricalCriticalVulnerabilityCount,
    HistoricalWarningCount,
    HistoricalNoticeWarningCount,
    HistoricalUnmaintainedWarningCount,
    HistoricalUnsoundWarningCount,
    HistoricalYankedWarningCount,
}

impl Metric {
    #[must_use]
    pub const fn category(self) -> MetricCategory {
        match self {
            Self::Age | Self::MinVersion | Self::ReleaseCount => MetricCategory::Stability,

            Self::OverallDownloadCount | Self::OneMonthDownloadCount | Self::DependentCount => MetricCategory::Usage,

            Self::RepoStarCount | Self::RepoForkCount | Self::RepoSubscriberCount | Self::RepoContributorCount => MetricCategory::Community,

            Self::CommitActivity
            | Self::OpenIssueCount
            | Self::ClosedIssueCount
            | Self::IssueResponsiveness
            | Self::OpenPullRequestCount
            | Self::ClosedPullRequestCount
            | Self::PullRequestResponsiveness => MetricCategory::Activity,

            Self::DocCoveragePercentage | Self::BrokenDocLinkCount | Self::ExampleCount => MetricCategory::Documentation,

            Self::OverallOwnerCount | Self::UserOwnerCount | Self::TeamOwnerCount | Self::License => MetricCategory::Ownership,

            Self::CodeCoveragePercentage | Self::FullySafeCode => MetricCategory::Trustworthiness,

            Self::TransitiveDependencyCount | Self::DirectDependencyCount => MetricCategory::Cost,

            Self::VulnerabilityCount
            | Self::LowVulnerabilityCount
            | Self::MediumVulnerabilityCount
            | Self::HighVulnerabilityCount
            | Self::CriticalVulnerabilityCount
            | Self::WarningCount
            | Self::NoticeWarningCount
            | Self::UnmaintainedWarningCount
            | Self::UnsoundWarningCount
            | Self::YankedWarningCount
            | Self::HistoricalVulnerabilityCount
            | Self::HistoricalLowVulnerabilityCount
            | Self::HistoricalMediumVulnerabilityCount
            | Self::HistoricalHighVulnerabilityCount
            | Self::HistoricalCriticalVulnerabilityCount
            | Self::HistoricalWarningCount
            | Self::HistoricalNoticeWarningCount
            | Self::HistoricalUnmaintainedWarningCount
            | Self::HistoricalUnsoundWarningCount
            | Self::HistoricalYankedWarningCount => MetricCategory::Advisories,
        }
    }
}
