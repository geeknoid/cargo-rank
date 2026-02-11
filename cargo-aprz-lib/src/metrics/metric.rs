use super::MetricCategory;
use super::MetricValue;
use super::metric_def::{METRIC_DEFINITIONS, MetricDef};
use crate::facts::CrateFacts;

#[cfg(test)]
use crate::facts::{CratesData, DocMetricState, ProviderResult};

#[cfg(test)]
use crate::facts::advisories::AdvisoryData;
#[cfg(test)]
use crate::facts::codebase::CodebaseData;
#[cfg(test)]
use crate::facts::coverage::CoverageData;
#[cfg(test)]
use crate::facts::crates::{CrateOverallData, CrateVersionData};
#[cfg(test)]
use crate::facts::docs::DocsData;
#[cfg(test)]
use crate::facts::hosting::HostingData;

#[derive(Debug, Clone)]
pub struct Metric {
    pub def: &'static MetricDef,
    pub value: Option<MetricValue>,
}

impl Metric {
    #[must_use]
    pub const fn new(def: &'static MetricDef) -> Self {
        Self { def, value: None }
    }

    #[must_use]
    pub const fn with_value(def: &'static MetricDef, value: MetricValue) -> Self {
        Self { def, value: Some(value) }
    }

    // Convenience accessors for common fields
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.def.name
    }

    #[must_use]
    pub const fn description(&self) -> &'static str {
        self.def.description
    }

    #[must_use]
    pub const fn category(&self) -> MetricCategory {
        self.def.category
    }
}

/// Flatten `CrateFacts` into an iterator of metrics with descriptive names
pub fn flatten(facts: &CrateFacts) -> impl Iterator<Item = Metric> + '_ {
    METRIC_DEFINITIONS
        .iter()
        .map(|def| (def.extractor)(facts).map_or_else(|| Metric::new(def), |value| Metric::with_value(def, value)))
}

/// Return an iterator of all known metrics with default values
///
/// This is useful for validation and testing purposes where you need metrics
/// with placeholder values to evaluate expressions against.
pub fn default_metrics() -> impl Iterator<Item = Metric> {
    METRIC_DEFINITIONS
        .iter()
        .map(|def| (def.default_value)().map_or_else(|| Metric::new(def), |value| Metric::with_value(def, value)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::CrateSpec;
    use crate::facts::advisories::AdvisoryCounts;
    use crate::facts::docs::DocsMetrics;
    use crate::facts::hosting::{AgeStats, IssueStats};
    use chrono::Utc;
    use semver::Version;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    #[expect(clippy::too_many_lines, reason = "Test helper function with comprehensive test data")]
    fn create_test_crate_facts() -> CrateFacts {
        use chrono::TimeZone;
        let now = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();

        CrateFacts {
            crate_spec: CrateSpec::from_arcs(Arc::from("test-crate"), Arc::new(Version::parse("1.0.0").unwrap())),
            crates_data: ProviderResult::Found(CratesData::new(
                CrateVersionData {
                    description: "Test crate".into(),
                    homepage: None,
                    documentation: None,
                    license: "MIT".into(),
                    rust_version: "1.70.0".into(),
                    edition: None,
                    features: BTreeMap::new(),
                    created_at: now,
                    updated_at: now,
                    yanked: false,
                    downloads: 1000,
                    monthly_downloads: vec![],
                },
                CrateOverallData {
                    created_at: now,
                    updated_at: now,
                    repository: None,
                    categories: vec![],
                    keywords: vec!["test".into()],
                    owners: vec![],
                    monthly_downloads: vec![],
                    downloads: 5000,
                    dependents: 10,
                    versions_last_90_days: 0,
                },
            )),
            hosting_data: ProviderResult::Found(HostingData {
                timestamp: now,
                stars: 100,
                forks: 20,
                subscribers: 5,
                issues: IssueStats {
                    open_count: 5,
                    closed_count: 20,
                    open_age: AgeStats {
                        avg: 10,
                        p50: 8,
                        p75: 15,
                        p90: 20,
                        p95: 25,
                    },
                    closed_age: AgeStats::default(),
                },
                pulls: IssueStats {
                    open_count: 2,
                    closed_count: 15,
                    open_age: AgeStats {
                        avg: 5,
                        p50: 4,
                        p75: 7,
                        p90: 10,
                        p95: 12,
                    },
                    closed_age: AgeStats::default(),
                },
            }),
            advisory_data: ProviderResult::Found(AdvisoryData {
                timestamp: now,
                per_version: AdvisoryCounts::default(),
                total: AdvisoryCounts {
                    low_vulnerability_count: 1,
                    medium_vulnerability_count: 0,
                    high_vulnerability_count: 0,
                    critical_vulnerability_count: 0,
                    notice_warning_count: 0,
                    unmaintained_warning_count: 0,
                    unsound_warning_count: 0,
                    yanked_warning_count: 0,
                },
            }),
            codebase_data: ProviderResult::Found(CodebaseData {
                timestamp: now,
                source_files_analyzed: 10,
                source_files_with_errors: 0,
                production_lines: 1000,
                test_lines: 500,
                comment_lines: 200,
                unsafe_count: 2,
                example_count: 3,
                transitive_dependencies: 25,
                workflows_detected: true,
                miri_detected: false,
                clippy_detected: true,
                contributors: 5,
                commits_last_90_days: 50,
                commits_last_365_days: 200,
                commit_count: 1000,
                last_commit_at: now,
            }),
            coverage_data: ProviderResult::Found(CoverageData {
                timestamp: now,
                code_coverage_percentage: 85.5,
            }),
            docs_data: ProviderResult::Found(DocsData {
                timestamp: now,
                metrics: DocMetricState::Found(DocsMetrics {
                    doc_coverage_percentage: 90.0,
                    public_api_elements: 100,
                    undocumented_elements: 10,
                    examples_in_docs: 25,
                    has_crate_level_docs: true,
                    broken_doc_links: 1,
                }),
            }),
        }
    }

    #[test]
    fn test_flatten_returns_metrics() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        // Should have a substantial number of metrics
        assert!(metrics.len() > 50, "Expected many metrics, got {}", metrics.len());

        // All metrics should have values
        for metric in &metrics {
            assert!(metric.value.is_some(), "Metric '{}' should have a value", metric.name());
        }
    }

    #[test]
    fn test_flatten_includes_version_data() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        // Check that version-related metrics exist
        assert!(metrics.iter().any(|m| m.name() == "crate.version"), "Should have version metric");
        assert!(
            metrics.iter().any(|m| m.name() == "usage.version_downloads"),
            "Should have version downloads metric"
        );
        assert!(metrics.iter().any(|m| m.name() == "crate.license"), "Should have license metric");
    }

    #[test]
    fn test_flatten_includes_overall_data() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        assert!(metrics.iter().any(|m| m.name() == "crate.name"), "Should have crate name metric");
        assert!(
            metrics.iter().any(|m| m.name() == "usage.total_downloads"),
            "Should have total downloads metric"
        );
        assert!(
            metrics.iter().any(|m| m.name() == "usage.dependent_crates"),
            "Should have dependent crate count metric"
        );
    }

    #[test]
    fn test_flatten_includes_hosting_data() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        assert!(
            metrics.iter().any(|m| m.name() == "community.repo_stars"),
            "Should have stars metric"
        );
        assert!(
            metrics.iter().any(|m| m.name() == "activity.open_issues"),
            "Should have open issues metric"
        );
        assert!(
            metrics.iter().any(|m| m.name() == "activity.commits_last_90_days"),
            "Should have commits metric"
        );
    }

    #[test]
    fn test_flatten_includes_advisory_data() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        assert!(
            metrics
                .iter()
                .any(|m| m.name() == "advisories.version_low_severity_vulnerabilities"),
            "Should have version low severity vulnerabilities metric"
        );
        assert!(
            metrics
                .iter()
                .any(|m| m.name() == "advisories.total_critical_severity_vulnerabilities"),
            "Should have total critical severity vulnerabilities metric"
        );
        assert!(
            metrics.iter().any(|m| m.name() == "advisories.version_notice_warnings"),
            "Should have version notice warnings metric"
        );
    }

    #[test]
    fn test_flatten_includes_codebase_data() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        assert!(
            metrics.iter().any(|m| m.name() == "code.code_lines"),
            "Should have production lines metric"
        );
        assert!(
            metrics.iter().any(|m| m.name() == "trust.unsafe_blocks"),
            "Should have unsafe count metric"
        );
        assert!(
            metrics.iter().any(|m| m.name() == "trust.ci_workflows"),
            "Should have CI workflows metric"
        );
    }

    #[test]
    fn test_flatten_includes_coverage_data() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        assert!(
            metrics.iter().any(|m| m.name() == "trust.code_coverage_percentage"),
            "Should have code coverage metric"
        );
    }

    #[test]
    fn test_flatten_includes_docs_data() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        assert!(
            metrics.iter().any(|m| m.name() == "docs.public_api_coverage_percentage"),
            "Should have doc coverage metric"
        );
        assert!(
            metrics.iter().any(|m| m.name() == "docs.public_api_elements"),
            "Should have public API elements metric"
        );
        assert!(
            metrics.iter().any(|m| m.name() == "docs.broken_links"),
            "Should have broken links metric"
        );
    }

    #[test]
    fn test_metric_categories_are_assigned() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        // Check that different categories are used
        let categories: std::collections::HashSet<_> = metrics.iter().map(Metric::category).collect();

        assert!(categories.len() > 5, "Should use multiple metric categories, found {categories:?}");
    }

    #[test]
    #[cfg_attr(coverage_nightly, coverage(off))]
    fn test_all_metrics_have_descriptions() {
        let facts = create_test_crate_facts();
        let metrics: Vec<_> = flatten(&facts).collect();

        for metric in &metrics {
            assert!(
                !metric.description().is_empty(),
                "Metric '{}' should have a description",
                metric.name()
            );
            assert!(
                metric.description().len() > 10,
                "Metric '{}' description should be meaningful",
                metric.name()
            );
        }
    }
}
