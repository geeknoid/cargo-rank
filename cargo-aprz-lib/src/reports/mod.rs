//! Multi-format report generation for crate analysis results
//!
//! This module provides report generators that transform crate metrics and
//! evaluation outcomes into various output formats for human consumption
//! and programmatic processing.
//!
//! # Implementation Model
//!
//! Five report generators are provided, each accessed through a `generate` function:
//! - **Console**: Terminal output with ANSI colors and Unicode box drawing
//! - **CSV**: Spreadsheet-compatible format with proper escaping
//! - **HTML**: Self-contained document with embedded CSS, dark mode, and search
//! - **Excel**: Native .xlsx format with multiple sheets and formatting
//! - **JSON**: Machine-readable structured data
//!
//! All generators operate on the same input: a slice of `ReportableCrate` containing
//! crate information, metrics, and optional evaluation outcomes. This uniform interface
//! allows callers to generate multiple report formats from the same data.
//!
//! Common functionality is centralized in the `common` module:
//! - Metric formatting (pretty-printing values with appropriate precision)
//! - Sorting (by crate name and version)
//! - Categorization (grouping metrics by `MetricCategory`)
//! - Status formatting (acceptance status display)
//!
//! The generators support optional evaluation displays based
//! on evaluation outcomes.

mod common;
mod console;
mod csv;
mod excel;
mod html;
mod json;
mod reportable_crate;

pub use console::generate as generate_console;
pub use csv::generate as generate_csv;
pub use excel::generate as generate_xlsx;
pub use html::generate as generate_html;
pub use json::generate as generate_json;
pub use reportable_crate::ReportableCrate;

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crate::expr::{Appraisal, ExpressionOutcome, Risk};
    use crate::metrics::{Metric, MetricCategory, MetricDef, MetricValue};
    use chrono::{DateTime, Local, TimeZone, Utc};
    use semver::Version;
    use std::io::Cursor;

    fn test_timestamp() -> DateTime<Local> {
        Local.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap()
    }

    // Define test metric definitions
    static NAME_DEF: MetricDef = MetricDef {
        name: "crate.name",
        description: "Name of the crate",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    static VERSION_DEF: MetricDef = MetricDef {
        name: "crate.version",
        description: "Version of the crate",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    static STARS_DEF: MetricDef = MetricDef {
        name: "community.repo_stars",
        description: "Number of stars",
        category: MetricCategory::Community,
        extractor: |_| None,
        default_value: || None,
    };

    static DOWNLOADS_DEF: MetricDef = MetricDef {
        name: "usage.total_downloads",
        description: "Total downloads",
        category: MetricCategory::Usage,
        extractor: |_| None,
        default_value: || None,
    };

    static COVERAGE_DEF: MetricDef = MetricDef {
        name: "trust.code_coverage_percentage",
        description: "Code coverage percentage",
        category: MetricCategory::Trustworthiness,
        extractor: |_| None,
        default_value: || None,
    };

    static CREATED_AT_DEF: MetricDef = MetricDef {
        name: "stability.crate_created_at",
        description: "When the crate was created",
        category: MetricCategory::Stability,
        extractor: |_| None,
        default_value: || None,
    };

    static HAS_CI_DEF: MetricDef = MetricDef {
        name: "trust.ci_workflows",
        description: "Whether CI is configured",
        category: MetricCategory::Trustworthiness,
        extractor: |_| None,
        default_value: || None,
    };

    static KEYWORDS_DEF: MetricDef = MetricDef {
        name: "crate.keywords",
        description: "Crate keywords",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    fn create_test_crates() -> Vec<ReportableCrate> {
        let created_at = Utc.with_ymd_and_hms(2023, 1, 15, 10, 30, 0).unwrap();

        vec![
            ReportableCrate::new(
                "tokio".to_string(),
                Version::parse("1.35.0").unwrap(),
                vec![
                    Metric::with_value(&NAME_DEF, MetricValue::String("tokio".into())),
                    Metric::with_value(&VERSION_DEF, MetricValue::String("1.35.0".into())),
                    Metric::with_value(&STARS_DEF, MetricValue::UInt(20000)),
                    Metric::with_value(&DOWNLOADS_DEF, MetricValue::UInt(50_000_000)),
                    Metric::with_value(&COVERAGE_DEF, MetricValue::Float(85.5)),
                    Metric::with_value(&CREATED_AT_DEF, MetricValue::DateTime(created_at)),
                    Metric::with_value(&HAS_CI_DEF, MetricValue::Boolean(true)),
                    Metric::with_value(
                        &KEYWORDS_DEF,
                        MetricValue::List(vec![MetricValue::String("async".into()), MetricValue::String("runtime".into())]),
                    ),
                ],
                Some(Appraisal::new(Risk::Low, vec![ExpressionOutcome::new("high_stars".to_string(), "High stars and good coverage".to_string(), true)])),
            ),
            ReportableCrate::new(
                "serde".to_string(),
                Version::parse("1.0.195").unwrap(),
                vec![
                    Metric::with_value(&NAME_DEF, MetricValue::String("serde".into())),
                    Metric::with_value(&VERSION_DEF, MetricValue::String("1.0.195".into())),
                    Metric::with_value(&STARS_DEF, MetricValue::UInt(8000)),
                    Metric::with_value(&DOWNLOADS_DEF, MetricValue::UInt(100_000_000)),
                    Metric::with_value(&COVERAGE_DEF, MetricValue::Float(92.3)),
                    Metric::with_value(&CREATED_AT_DEF, MetricValue::DateTime(created_at)),
                    Metric::with_value(&HAS_CI_DEF, MetricValue::Boolean(true)),
                    Metric::with_value(&KEYWORDS_DEF, MetricValue::List(vec![MetricValue::String("serialization".into())])),
                ],
                Some(Appraisal::new(Risk::High, vec![ExpressionOutcome::new("low_stars".to_string(), "Low star count".to_string(), false)])),
            ),
            ReportableCrate::new(
                "anyhow".to_string(),
                Version::parse("1.0.75").unwrap(),
                vec![
                    Metric::with_value(&NAME_DEF, MetricValue::String("anyhow".into())),
                    Metric::with_value(&VERSION_DEF, MetricValue::String("1.0.75".into())),
                    Metric::with_value(&STARS_DEF, MetricValue::UInt(4500)),
                    Metric::with_value(&DOWNLOADS_DEF, MetricValue::UInt(30_000_000)),
                    Metric::with_value(&COVERAGE_DEF, MetricValue::Float(78.9)),
                    Metric::with_value(&CREATED_AT_DEF, MetricValue::DateTime(created_at)),
                    Metric::with_value(&HAS_CI_DEF, MetricValue::Boolean(false)),
                    Metric::with_value(
                        &KEYWORDS_DEF,
                        MetricValue::List(vec![MetricValue::String("error".into()), MetricValue::String("handling".into())]),
                    ),
                ],
                None,
            ),
        ]
    }

    #[test]
    fn test_console_report_with_colors() {
        let crates = create_test_crates();
        let mut output = String::new();
        generate_console(&crates, true, &mut output).unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_console_report_no_colors() {
        let crates = create_test_crates();
        let mut output = String::new();
        generate_console(&crates, false, &mut output).unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_csv_report() {
        let crates = create_test_crates();
        let mut output = String::new();
        generate_csv(&crates, &mut output).unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_html_report() {
        let crates = create_test_crates();
        let mut output = String::new();
        generate_html(&crates, test_timestamp(), &mut output).unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_json_report() {
        let crates = create_test_crates();
        let mut output = String::new();
        generate_json(&crates, &mut output).unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_excel_report() {
        let crates = create_test_crates();
        let mut output = Cursor::new(Vec::new());
        generate_xlsx(&crates, &mut output).unwrap();

        // For Excel, we just verify it doesn't error and produces non-empty output
        // Full snapshot testing of binary Excel files isn't practical
        let bytes = output.into_inner();
        assert!(!bytes.is_empty(), "Excel output should not be empty");
        assert!(bytes.len() > 1000, "Excel output should be substantial");

        // Verify it starts with the Excel magic number (PK for ZIP format)
        assert_eq!(&bytes[0..2], b"PK", "Excel file should be a valid ZIP archive");
    }

    #[test]
    fn test_empty_crates_console() {
        let crates: Vec<ReportableCrate> = vec![];
        let mut output = String::new();
        generate_console(&crates, false, &mut output).unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_empty_crates_csv() {
        let crates: Vec<ReportableCrate> = vec![];
        let mut output = String::new();
        generate_csv(&crates, &mut output).unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_empty_crates_json() {
        let crates: Vec<ReportableCrate> = vec![];
        let mut output = String::new();
        generate_json(&crates, &mut output).unwrap();
        insta::assert_snapshot!(output);
    }

    #[test]
    fn test_single_crate_all_metrics() {
        let created_at = Utc.with_ymd_and_hms(2023, 6, 1, 12, 0, 0).unwrap();

        let crate_with_all_metrics = ReportableCrate::new(
            "comprehensive-crate".to_string(),
            Version::parse("2.0.0").unwrap(),
            vec![
                Metric::with_value(&NAME_DEF, MetricValue::String("comprehensive-crate".into())),
                Metric::with_value(&VERSION_DEF, MetricValue::String("2.0.0".into())),
                Metric::with_value(&STARS_DEF, MetricValue::UInt(12345)),
                Metric::with_value(&DOWNLOADS_DEF, MetricValue::UInt(9_876_543)),
                Metric::with_value(&COVERAGE_DEF, MetricValue::Float(99.99)),
                Metric::with_value(&CREATED_AT_DEF, MetricValue::DateTime(created_at)),
                Metric::with_value(&HAS_CI_DEF, MetricValue::Boolean(true)),
                Metric::with_value(
                    &KEYWORDS_DEF,
                    MetricValue::List(vec![
                        MetricValue::String("test".into()),
                        MetricValue::String("comprehensive".into()),
                        MetricValue::String("snapshot".into()),
                    ]),
                ),
            ],
            Some(Appraisal::new(
                Risk::Low,
                vec![
                    ExpressionOutcome::new("coverage".to_string(), "Excellent coverage".to_string(), true),
                    ExpressionOutcome::new("active".to_string(), "Active development".to_string(), true),
                    ExpressionOutcome::new("maintained".to_string(), "Well maintained".to_string(), true),
                ],
            )),
        );

        let crates = vec![crate_with_all_metrics];

        // Test in all formats
        let mut console_output = String::new();
        generate_console(&crates, false, &mut console_output).unwrap();
        insta::assert_snapshot!("single_crate_console", console_output);

        let mut csv_output = String::new();
        generate_csv(&crates, &mut csv_output).unwrap();
        insta::assert_snapshot!("single_crate_csv", csv_output);

        let mut json_output = String::new();
        generate_json(&crates, &mut json_output).unwrap();
        insta::assert_snapshot!("single_crate_json", json_output);
    }
}
