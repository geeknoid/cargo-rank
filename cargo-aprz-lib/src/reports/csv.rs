use super::{ReportableCrate, common};
use crate::Result;
use crate::metrics::MetricCategory;
use core::fmt::Write;
use std::borrow::Cow;
use strum::IntoEnumIterator;

pub fn generate<W: Write>(crates: &[ReportableCrate], writer: &mut W) -> Result<()> {
    // Group metrics by category across all crates
    let metrics_by_category = common::group_all_metrics_by_category(crates.iter().map(|c| c.metrics.as_slice()));

    // Build per-crate metric lookup maps for O(1) access
    let crate_metric_maps = common::build_metric_lookup_maps(crates);

    // Write header row
    write!(writer, "Metric")?;
    for crate_info in crates {
        write!(writer, ",{}", escape_csv(&format!("{} v{}", crate_info.name, crate_info.version)))?;
    }
    writeln!(writer)?;

    // Write appraisal rows if any crate has an appraisal
    let has_appraisals = crates.iter().any(|c| c.appraisal.is_some());
    if has_appraisals {
        write!(writer, "Appraisals")?;
        for crate_info in crates {
            if let Some(eval) = &crate_info.appraisal {
                let status_str = common::format_appraisal_status(eval);
                write!(writer, ",{status_str}")?;
            } else {
                write!(writer, ",")?;
            }
        }
        writeln!(writer)?;

        write!(writer, "Reasons")?;
        for crate_info in crates {
            if let Some(appraisal) = &crate_info.appraisal {
                let reasons = common::join_with(
                    appraisal.expression_outcomes.iter().map(common::outcome_icon_name), "; ");
                write!(writer, ",{}", escape_csv(&reasons))?;
            } else {
                write!(writer, ",")?;
            }
        }
        writeln!(writer)?;
    }

    // Write metrics grouped by category
    let mut metric_buf = String::new();
    for category in MetricCategory::iter() {
        if let Some(category_metrics) = metrics_by_category.get(&category) {
            // Write each metric in this category
            for metric_name in category_metrics {
                write!(writer, "{}", escape_csv(metric_name))?;

                // Write values for each crate
                for metric_map in &crate_metric_maps {
                    if let Some(metric) = metric_map.get(metric_name)
                        && let Some(ref value) = metric.value
                    {
                        metric_buf.clear();
                        common::write_metric_value(&mut metric_buf, value);
                        write!(writer, ",{}", escape_csv(&metric_buf))?;
                    } else {
                        write!(writer, ",")?;
                    }
                }
                writeln!(writer)?;
            }
        }
    }

    Ok(())
}

/// Escape a value for RFC compliant CSV output.
///
/// Wraps the value in double quotes if it contains commas, newlines, or double quotes.
/// Internal double quotes are doubled per the RFC.
fn escape_csv(s: &str) -> Cow<'_, str> {
    if s.contains('"') {
        Cow::Owned(format!("\"{}\"", s.replace('"', "\"\"")))
    } else if s.contains(',') || s.contains('\n') || s.contains('\r') {
        Cow::Owned(format!("\"{s}\""))
    } else {
        Cow::Borrowed(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{Appraisal, ExpressionDisposition, ExpressionOutcome, Risk};
    use crate::metrics::{Metric, MetricDef, MetricValue};
    use std::sync::Arc;

    static NAME_DEF: MetricDef = MetricDef {
        name: "name",
        description: "Crate name",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    static VERSION_DEF: MetricDef = MetricDef {
        name: "version",
        description: "Crate version",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    fn create_test_crate(name: &str, version: &str, evaluation: Option<Appraisal>) -> ReportableCrate {
        let metrics = vec![
            Metric::with_value(&NAME_DEF, MetricValue::String(name.into())),
            Metric::with_value(&VERSION_DEF, MetricValue::String(version.into())),
        ];
        ReportableCrate::new(name.into(), Arc::new(version.parse().unwrap()), metrics, evaluation)
    }

    #[test]
    fn test_escape_csv_no_special_chars() {
        let result = escape_csv("hello world");
        assert_eq!(result, "hello world");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn test_escape_csv_with_quotes() {
        let result = escape_csv("hello \"world\"");
        assert_eq!(result, "\"hello \"\"world\"\"\"");
        assert!(matches!(result, Cow::Owned(_)));
    }

    #[test]
    fn test_escape_csv_with_comma() {
        let result = escape_csv("hello,world");
        assert_eq!(result, "\"hello,world\"");
        assert!(matches!(result, Cow::Owned(_)));
    }

    #[test]
    fn test_escape_csv_with_newline() {
        let result = escape_csv("hello\nworld");
        assert_eq!(result, "\"hello\nworld\"");
        assert!(matches!(result, Cow::Owned(_)));
    }

    #[test]
    fn test_escape_csv_empty() {
        let result = escape_csv("");
        assert_eq!(result, "");
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn test_generate_empty_crates() {
        let crates: Vec<ReportableCrate> = vec![];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        // Should only have header
        assert_eq!(output, "Metric\n");
    }

    #[test]
    fn test_generate_single_crate_no_evaluation() {
        let crates = vec![create_test_crate("test_crate", "1.2.3", None)];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        // Should have header with crate name and version
        assert!(output.starts_with("Metric,test_crate v1.2.3"));
        // Should not have Status or Reasons rows
        assert!(!output.contains("Status,"));
        assert!(!output.contains("Reasons,"));
    }

    #[test]
    fn test_generate_single_crate_with_evaluation() {
        let eval = Appraisal {
            risk: Risk::Low,
            expression_outcomes: vec![
                ExpressionOutcome::new("good".into(), "Good".into(), ExpressionDisposition::True),
                ExpressionOutcome::new("quality".into(), "Quality".into(), ExpressionDisposition::True),
            ],
            available_points: 2,
            awarded_points: 2,
            score: 100.0,
        };
        let crates = vec![create_test_crate("test_crate", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        assert!(output.contains("Appraisals,LOW RISK"));
        assert!(output.contains("Reasons,✔\u{fe0f} good; ✔\u{fe0f} quality"));
    }

    #[test]
    fn test_generate_multiple_crates() {
        let crates = vec![
            create_test_crate("crate_a", "1.0.0", None),
            create_test_crate("crate_b", "2.0.0", None),
        ];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        // Should have both crates in header
        assert!(output.contains("crate_a v1.0.0"));
        assert!(output.contains("crate_b v2.0.0"));
    }

    #[test]
    fn test_generate_with_special_characters() {
        let eval = Appraisal {
            risk: Risk::Low,
            expression_outcomes: vec![ExpressionOutcome::new("quotes".into(), "Reason with \"quotes\"".into(), ExpressionDisposition::True)],
            available_points: 1,
            awarded_points: 1,
            score: 100.0,
        };
        let crates = vec![create_test_crate("test,\"crate\"", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        // Name with quotes in crate name should be escaped
        assert!(output.contains("test,"));
    }

    #[test]
    fn test_generate_denied_status() {
        let eval = Appraisal {
            risk: Risk::High,
            expression_outcomes: vec![ExpressionOutcome::new("security".into(), "Security issue".into(), ExpressionDisposition::False)],
            available_points: 1,
            awarded_points: 0,
            score: 0.0,
        };
        let crates = vec![create_test_crate("bad_crate", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        assert!(output.contains("Appraisals,HIGH RISK"));
    }
}
