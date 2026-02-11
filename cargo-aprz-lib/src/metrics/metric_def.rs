use super::{MetricCategory, MetricValue};
use crate::facts::{CrateFacts, DocMetricState};
use chrono::DateTime;
use compact_str::format_compact;

#[derive(Debug)]
pub struct MetricDef {
    pub name: &'static str,
    pub description: &'static str,
    pub category: MetricCategory,
    pub extractor: fn(&CrateFacts) -> Option<MetricValue>,
    pub default_value: fn() -> Option<MetricValue>,
}

macro_rules! metric_def {
    ($name:expr, $description:expr, $category:ident, $extractor:expr, $default:expr) => {
        MetricDef {
            name: $name,
            description: $description,
            category: MetricCategory::$category,
            extractor: $extractor,
            default_value: $default,
        }
    };
}

fn calculate_recent_downloads(monthly_downloads: &[(chrono::NaiveDate, u64)]) -> u64 {
    monthly_downloads.iter().rev().take(3).map(|(_, count)| count).sum()
}

pub const METRIC_DEFINITIONS: &[MetricDef] = &[
    metric_def!(
        "crate.name",
        "Name of the crate",
        Metadata,
        |facts| Some(MetricValue::String(facts.crate_spec.name().into())),
        || Some(MetricValue::String("".into()))
    ),
    metric_def!(
        "crate.version",
        "Semantic version of the crate",
        Metadata,
        |facts| Some(MetricValue::String(facts.crate_spec.version().to_string().into())),
        || Some(MetricValue::String("".into()))
    ),
    metric_def!(
        "crate.description",
        "Description of the crate's purpose and use",
        Metadata,
        |facts| {
            facts
                .crates_data
                .as_ref()
                .map(|data| MetricValue::String(data.version_data.description.clone()))
        },
        || Some(MetricValue::String("".into()))
    ),
    metric_def!(
        "crate.license",
        "SPDX license identifier constraining use of the crate",
        Metadata,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::String(data.version_data.license.clone())),
        || Some(MetricValue::String("".into()))
    ),
    metric_def!(
        "crate.categories",
        "Crate categories",
        Metadata,
        |facts| facts.crates_data.as_ref().map(|data| MetricValue::List(
            data.overall_data
                .categories
                .iter()
                .map(|s| MetricValue::String(s.clone()))
                .collect()
        )),
        || Some(MetricValue::List(Vec::new()))
    ),
    metric_def!(
        "crate.keywords",
        "Crate keywords",
        Metadata,
        |facts| {
            facts
                .crates_data
                .as_ref()
                .map(|data| MetricValue::List(data.overall_data.keywords.iter().map(|s| MetricValue::String(s.clone())).collect()))
        },
        || Some(MetricValue::List(Vec::new()))
    ),
    metric_def!(
        "crate.features",
        "Available crate features",
        Metadata,
        |facts| facts.crates_data.as_ref().map(|data| MetricValue::List(
            data.version_data
                .features
                .keys()
                .map(|s| MetricValue::String(s.clone()))
                .collect()
        )),
        || Some(MetricValue::List(Vec::new()))
    ),
    metric_def!(
        "crate.repository",
        "URL to the crate's source code repository",
        Metadata,
        |facts| {
            facts.crates_data.as_ref().map(|data| {
                MetricValue::String(
                    data.overall_data
                        .repository
                        .as_ref()
                        .map_or_else(|| "".into(), |url| url.as_str().into()),
                )
            })
        },
        || Some(MetricValue::String("".into()))
    ),
    metric_def!(
        "crate.homepage",
        "URL to the crate's homepage",
        Metadata,
        |facts| facts.crates_data.as_ref().map(|data| MetricValue::String(
            data.version_data
                .homepage
                .as_ref()
                .map_or_else(|| "".into(), |url| url.as_str().into())
        )),
        || Some(MetricValue::String("".into()))
    ),
    metric_def!(
        "crate.minimum_rust",
        "Minimum Rust version (MSRV) required to compile this crate",
        Metadata,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::String(data.version_data.rust_version.clone())),
        || Some(MetricValue::String("".into()))
    ),
    metric_def!(
        "crate.rust_edition",
        "Rust edition this crate targets",
        Metadata,
        |facts| facts.crates_data.as_ref().map(|data| MetricValue::String(
            data.version_data
                .edition
                .as_ref()
                .map_or_else(|| "".into(), |e| e.as_str().into())
        )),
        || Some(MetricValue::String("".into()))
    ),
    metric_def!(
        "docs.documentation",
        "URL to the crate's documentation",
        Documentation,
        |facts| {
            facts.crates_data.as_ref().map(|data| {
                let docs_url = data.version_data.documentation.as_ref().map_or_else(
                    || format_compact!("https://docs.rs/{}/{}", facts.crate_spec.name(), facts.crate_spec.version()),
                    |url| url.as_str().into(),
                );
                MetricValue::String(docs_url)
            })
        },
        || Some(MetricValue::String("".into()))
    ),
    metric_def!(
        "docs.public_api_elements",
        "Number of public API elements (functions, structs, etc.)",
        Documentation,
        |facts| {
            let data = facts.docs_data.as_ref()?;
            match &data.metrics {
                DocMetricState::Found(metrics) => Some(MetricValue::UInt(metrics.public_api_elements)),
                DocMetricState::UnknownFormatVersion(_) => None,
            }
        },
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "docs.undocumented_public_api_elements",
        "Number of public API elements without documentation",
        Documentation,
        |facts| {
            let data = facts.docs_data.as_ref()?;
            match &data.metrics {
                DocMetricState::Found(metrics) => Some(MetricValue::UInt(metrics.undocumented_elements)),
                DocMetricState::UnknownFormatVersion(_) => None,
            }
        },
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "docs.public_api_coverage_percentage",
        "Percentage of public API elements with documentation",
        Documentation,
        |facts| {
            let data = facts.docs_data.as_ref()?;
            match &data.metrics {
                DocMetricState::Found(metrics) => Some(MetricValue::Float(metrics.doc_coverage_percentage)),
                DocMetricState::UnknownFormatVersion(_) => None,
            }
        },
        || Some(MetricValue::Float(0.0))
    ),
    metric_def!(
        "docs.crate_level_docs_present",
        "Whether crate-level documentation exists",
        Documentation,
        |facts| {
            let data = facts.docs_data.as_ref()?;
            match &data.metrics {
                DocMetricState::Found(metrics) => Some(MetricValue::Boolean(metrics.has_crate_level_docs)),
                DocMetricState::UnknownFormatVersion(_) => None,
            }
        },
        || Some(MetricValue::Boolean(false))
    ),
    metric_def!(
        "docs.broken_links",
        "Number of broken links in documentation",
        Documentation,
        |facts| {
            let data = facts.docs_data.as_ref()?;
            match &data.metrics {
                DocMetricState::Found(metrics) => Some(MetricValue::UInt(metrics.broken_doc_links)),
                DocMetricState::UnknownFormatVersion(_) => None,
            }
        },
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "docs.examples_in_docs",
        "Number of code examples in documentation",
        Documentation,
        |facts| {
            let data = facts.docs_data.as_ref()?;
            match &data.metrics {
                DocMetricState::Found(metrics) => Some(MetricValue::UInt(metrics.examples_in_docs)),
                DocMetricState::UnknownFormatVersion(_) => None,
            }
        },
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "docs.standalone_examples",
        "Number of standalone example programs in the codebase",
        Documentation,
        |facts| facts.codebase_data.as_ref().map(|data| MetricValue::UInt(data.example_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "usage.total_downloads",
        "Crate downloads across all versions",
        Usage,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.overall_data.downloads)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "usage.total_downloads_last_90_days",
        "Crate downloads across all versions in the last 90 days",
        Usage,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::UInt(calculate_recent_downloads(&data.overall_data.monthly_downloads))),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "usage.version_downloads",
        "Crate downloads of this specific version",
        Usage,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.version_data.downloads)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "usage.version_downloads_last_90_days",
        "Crate downloads of this specific version in the last 90 days",
        Usage,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::UInt(calculate_recent_downloads(&data.version_data.monthly_downloads))),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "usage.dependent_crates",
        "Number of unique crates that depend on this crate",
        Usage,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.overall_data.dependents)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "stability.crate_created_at",
        "When the crate was first published to crates.io",
        Stability,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::DateTime(data.overall_data.created_at)),
        || Some(MetricValue::DateTime(
            DateTime::from_timestamp(0, 0).expect("epoch timestamp is always valid")
        ))
    ),
    metric_def!(
        "stability.crate_updated_at",
        "When the crate's metadata was last updated on crates.io",
        Stability,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::DateTime(data.overall_data.updated_at)),
        || Some(MetricValue::DateTime(
            DateTime::from_timestamp(0, 0).expect("epoch timestamp is always valid")
        ))
    ),
    metric_def!(
        "stability.version_created_at",
        "When this version was first published to crates.io",
        Stability,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::DateTime(data.version_data.created_at)),
        || Some(MetricValue::DateTime(
            DateTime::from_timestamp(0, 0).expect("epoch timestamp is always valid")
        ))
    ),
    metric_def!(
        "stability.version_updated_at",
        "When this version's metadata was last updated on crates.io",
        Stability,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::DateTime(data.version_data.updated_at)),
        || Some(MetricValue::DateTime(
            DateTime::from_timestamp(0, 0).expect("epoch timestamp is always valid")
        ))
    ),
    metric_def!(
        "stability.yanked",
        "Whether this version has been yanked from crates.io",
        Stability,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::Boolean(data.version_data.yanked)),
        || Some(MetricValue::Boolean(false))
    ),
    metric_def!(
        "stability.versions_last_90_days",
        "Number of versions published in the last 90 days",
        Stability,
        |facts| facts
            .crates_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.overall_data.versions_last_90_days)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "crate.owners",
        "List of owner usernames",
        Metadata,
        |facts| facts.crates_data.as_ref().map(|data| MetricValue::List(
            data.overall_data
                .owners
                .iter()
                .map(|o| MetricValue::String(o.login.clone()))
                .collect()
        )),
        || Some(MetricValue::List(Vec::new()))
    ),
    metric_def!(
        "community.repo_stars",
        "Number of stars on the repository",
        Community,
        |facts| { facts.hosting_data.as_ref().map(|data| MetricValue::UInt(data.stars)) },
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "community.repo_forks",
        "Number of forks of the repository",
        Community,
        |facts| { facts.hosting_data.as_ref().map(|data| MetricValue::UInt(data.forks)) },
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "community.repo_subscribers",
        "Number of users watching/subscribing to the repository",
        Community,
        |facts| facts.hosting_data.as_ref().map(|data| MetricValue::UInt(data.subscribers)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "community.repo_contributors",
        "Number of contributors to the repository",
        Community,
        |facts| facts.codebase_data.as_ref().map(|data| MetricValue::UInt(data.contributors)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.commits_last_90_days",
        "Number of commits to the repository in the last 90 days",
        Activity,
        |facts| facts
            .codebase_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.commits_last_90_days)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.commits_last_365_days",
        "Number of commits to the repository in the last 365 days",
        Activity,
        |facts| facts
            .codebase_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.commits_last_365_days)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.commit_count",
        "Total number of commits in the repository",
        Activity,
        |facts| facts.codebase_data.as_ref().map(|data| MetricValue::UInt(data.commit_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.last_commit_at",
        "Timestamp of the most recent commit in the repository",
        Activity,
        |facts| facts.codebase_data.as_ref().map(|data| MetricValue::DateTime(data.last_commit_at)),
        || Some(MetricValue::DateTime(DateTime::UNIX_EPOCH))
    ),
    metric_def!(
        "activity.open_issues",
        "Number of currently open issues",
        Activity,
        |facts| facts.hosting_data.as_ref().map(|data| MetricValue::UInt(data.issues.open_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.closed_issues",
        "Total number of issues that have been closed (all time)",
        Activity,
        |facts| facts.hosting_data.as_ref().map(|data| MetricValue::UInt(data.issues.closed_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.avg_open_issue_age_days",
        "Average age in days of open issues",
        Activity,
        |facts| facts
            .hosting_data
            .as_ref()
            .map(|data| MetricValue::UInt(u64::from(data.issues.open_age.avg))),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.median_open_issue_age_days",
        "Median age in days of open issues (50th percentile)",
        Activity,
        |facts| facts
            .hosting_data
            .as_ref()
            .map(|data| MetricValue::UInt(u64::from(data.issues.open_age.p50))),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.p90_open_issue_age_days",
        "90th percentile age in days of open issues",
        Activity,
        |facts| facts
            .hosting_data
            .as_ref()
            .map(|data| MetricValue::UInt(u64::from(data.issues.open_age.p90))),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.open_pull_requests",
        "Number of currently open pull requests",
        Activity,
        |facts| facts.hosting_data.as_ref().map(|data| MetricValue::UInt(data.pulls.open_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.closed_pull_requests",
        "Total number of pull requests that have been closed (all time)",
        Activity,
        |facts| facts.hosting_data.as_ref().map(|data| MetricValue::UInt(data.pulls.closed_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.avg_open_pull_request_age_days",
        "Average age in days of open pull requests",
        Activity,
        |facts| facts
            .hosting_data
            .as_ref()
            .map(|data| MetricValue::UInt(u64::from(data.pulls.open_age.avg))),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.median_open_pull_request_age_days",
        "Median age in days of open pull requests (50th percentile)",
        Activity,
        |facts| facts
            .hosting_data
            .as_ref()
            .map(|data| MetricValue::UInt(u64::from(data.pulls.open_age.p50))),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "activity.p90_open_pull_request_age_days",
        "90th percentile age in days of open pull requests",
        Activity,
        |facts| facts
            .hosting_data
            .as_ref()
            .map(|data| MetricValue::UInt(u64::from(data.pulls.open_age.p90))),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.total_low_severity_vulnerabilities",
        "Number of low severity vulnerabilities across all versions",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.total.low_vulnerability_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.total_medium_severity_vulnerabilities",
        "Number of medium severity vulnerabilities across all versions",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.total.medium_vulnerability_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.total_high_severity_vulnerabilities",
        "Number of high severity vulnerabilities across all versions",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.total.high_vulnerability_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.total_critical_severity_vulnerabilities",
        "Number of critical severity vulnerabilities across all versions",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.total.critical_vulnerability_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.total_notice_warnings",
        "Number of notice warnings across all versions",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.total.notice_warning_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.total_unmaintained_warnings",
        "Number of unmaintained warnings across all versions",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.total.unmaintained_warning_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.total_unsound_warnings",
        "Number of unsound warnings across all versions",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.total.unsound_warning_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.version_low_severity_vulnerabilities",
        "Number of low severity vulnerabilities in this version",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.per_version.low_vulnerability_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.version_medium_severity_vulnerabilities",
        "Number of medium severity vulnerabilities in this version",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.per_version.medium_vulnerability_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.version_high_severity_vulnerabilities",
        "Number of high severity vulnerabilities in this version",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.per_version.high_vulnerability_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.version_critical_severity_vulnerabilities",
        "Number of critical severity vulnerabilities in this version",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.per_version.critical_vulnerability_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.version_notice_warnings",
        "Number of notice warnings for this version",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.per_version.notice_warning_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.version_unmaintained_warnings",
        "Number of unmaintained warnings for this version",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.per_version.unmaintained_warning_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "advisories.version_unsound_warnings",
        "Number of unsound warnings for this version",
        Advisories,
        |facts| facts
            .advisory_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.per_version.unsound_warning_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "code.source_files",
        "Number of source files",
        Codebase,
        |facts| facts
            .codebase_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.source_files_analyzed)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "code.source_files_with_errors",
        "Number of source files that had analysis errors",
        Codebase,
        |facts| facts
            .codebase_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.source_files_with_errors)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "code.code_lines",
        "Number of lines of production code (excluding tests)",
        Codebase,
        |facts| facts.codebase_data.as_ref().map(|data| MetricValue::UInt(data.production_lines)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "code.test_lines",
        "Number of lines of test code",
        Codebase,
        |facts| facts.codebase_data.as_ref().map(|data| MetricValue::UInt(data.test_lines)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "code.comment_lines",
        "Number of comment lines in the codebase",
        Codebase,
        |facts| { facts.codebase_data.as_ref().map(|data| MetricValue::UInt(data.comment_lines)) },
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "code.transitive_dependencies",
        "Number of transitive dependencies",
        Codebase,
        |facts| facts
            .codebase_data
            .as_ref()
            .map(|data| MetricValue::UInt(data.transitive_dependencies)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "trust.unsafe_blocks",
        "Number of unsafe blocks in the codebase",
        Trustworthiness,
        |facts| facts.codebase_data.as_ref().map(|data| MetricValue::UInt(data.unsafe_count)),
        || Some(MetricValue::UInt(0))
    ),
    metric_def!(
        "trust.ci_workflows",
        "Whether CI/CD workflows were detected in the repository",
        Trustworthiness,
        |facts| facts
            .codebase_data
            .as_ref()
            .map(|data| MetricValue::Boolean(data.workflows_detected)),
        || Some(MetricValue::Boolean(false))
    ),
    metric_def!(
        "trust.miri_usage",
        "Whether Miri is used in CI",
        Trustworthiness,
        |facts| facts.codebase_data.as_ref().map(|data| MetricValue::Boolean(data.miri_detected)),
        || Some(MetricValue::Boolean(false))
    ),
    metric_def!(
        "trust.clippy_usage",
        "Whether Clippy is used in CI",
        Trustworthiness,
        |facts| facts.codebase_data.as_ref().map(|data| MetricValue::Boolean(data.clippy_detected)),
        || Some(MetricValue::Boolean(false))
    ),
    metric_def!(
        "trust.code_coverage_percentage",
        "Percentage of code covered by tests",
        Trustworthiness,
        |facts| facts
            .coverage_data
            .as_ref()
            .map(|data| MetricValue::Float(data.code_coverage_percentage)),
        || Some(MetricValue::Float(0.0))
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_metrics_have_default_values() {
        for metric_def in METRIC_DEFINITIONS {
            let default = (metric_def.default_value)();
            assert!(default.is_some(), "Metric '{}' does not have a default value", metric_def.name);
        }
    }

    #[test]
    fn test_all_metric_names_are_unique() {
        let mut names = std::collections::HashSet::new();
        for metric_def in METRIC_DEFINITIONS {
            assert!(names.insert(metric_def.name), "Duplicate metric name found: '{}'", metric_def.name);
        }
    }

    #[test]
    fn test_all_metrics_have_descriptions() {
        for metric_def in METRIC_DEFINITIONS {
            assert!(
                !metric_def.description.is_empty(),
                "Metric '{}' has an empty description",
                metric_def.name
            );
        }
    }
}
