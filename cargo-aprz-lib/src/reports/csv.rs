use super::{ReportableCrate, common};
use crate::Result;
use crate::metrics::MetricCategory;
use core::fmt::Write;
use std::borrow::Cow;
use std::collections::HashMap;
use strum::IntoEnumIterator;

pub fn generate<W: Write>(crates: &[ReportableCrate], writer: &mut W) -> Result<()> {
    // Group metrics by category (use first crate's metrics as template)
    let metrics_by_category = crates.first().map_or_else(HashMap::default, |crate_info| {
        common::group_metrics_by_category(&crate_info.metrics)
    });

    // Write header row
    write!(writer, "Metric")?;
    for crate_info in crates {
        write!(writer, ",{}", escape_csv(&format!("{} v{}", crate_info.name, crate_info.version)))?;
    }
    writeln!(writer)?;

    // Write ranking rows if any crate has an evaluation
    let has_evaluations = crates.iter().any(|c| c.evaluation.is_some());
    if has_evaluations {
        write!(writer, "Status")?;
        for crate_info in crates {
            if let Some(eval) = &crate_info.evaluation {
                let status_str = common::format_acceptance_status(eval.accepted);
                write!(writer, ",{status_str}")?;
            } else {
                write!(writer, ",")?;
            }
        }
        writeln!(writer)?;

        write!(writer, "Reasons")?;
        for crate_info in crates {
            if let Some(eval) = &crate_info.evaluation {
                let reasons = eval.reasons.join("; ");
                write!(writer, ",{}", escape_csv(&reasons))?;
            } else {
                write!(writer, ",")?;
            }
        }
        writeln!(writer)?;
    }

    // Write metrics grouped by category
    for category in MetricCategory::iter() {
        if let Some(category_metrics) = metrics_by_category.get(&category) {
            // Write each metric in this category
            for metric_name in category_metrics {
                write!(writer, "{}", escape_csv(metric_name))?;

                // Write values for each crate
                for crate_info in crates {
                    if let Some(metric) = crate_info.metrics.iter().find(|m| m.name() == *metric_name)
                        && let Some(ref value) = metric.value
                    {
                        write!(writer, ",{}", escape_csv(&common::format_metric_value(value)))?;
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
    use crate::expr::EvaluationOutcome;
    use crate::metrics::{Metric, MetricDef, MetricValue};

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

    fn create_test_crate(name: &str, version: &str, evaluation: Option<EvaluationOutcome>) -> ReportableCrate {
        let metrics = vec![
            Metric::with_value(&NAME_DEF, MetricValue::String(name.into())),
            Metric::with_value(&VERSION_DEF, MetricValue::String(version.into())),
        ];
        ReportableCrate::new(name.to_string(), version.parse().unwrap(), metrics, evaluation)
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
    fn test_generate_single_crate_no_ranking() {
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
    fn test_generate_single_crate_with_ranking() {
        let eval = EvaluationOutcome {
            accepted: true,
            reasons: vec!["Good".to_string(), "Quality".to_string()],
        };
        let crates = vec![create_test_crate("test_crate", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        assert!(output.contains("Status,ACCEPTABLE"));
        assert!(output.contains("Reasons,Good; Quality"));
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
        let eval = EvaluationOutcome {
            accepted: true,
            reasons: vec!["Reason with \"quotes\"".to_string()],
        };
        let crates = vec![create_test_crate("test,\"crate\"", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        // Quotes should be escaped
        assert!(output.contains("\"\"quotes\"\""));
    }

    #[test]
    fn test_generate_denied_status() {
        let eval = EvaluationOutcome {
            accepted: false,
            reasons: vec!["Security issue".to_string()],
        };
        let crates = vec![create_test_crate("bad_crate", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        assert!(output.contains("Status,NOT ACCEPTABLE"));
    }
}
