use super::{ReportableCrate, common};
use crate::Result;
use crate::metrics::{Metric, MetricCategory};
use core::fmt::Write;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use strum::IntoEnumIterator;
use terminal_size::{Width, terminal_size};

pub fn generate<W: Write>(crates: &[ReportableCrate], use_colors: bool, writer: &mut W) -> Result<()> {
    for (index, crate_info) in crates.iter().enumerate() {
        if index > 0 {
            writeln!(writer)?;
            writeln!(writer, "═══════════════════════════════════════")?;
            writeln!(writer)?;
        }

        // Show acceptance status if evaluation was performed
        if let Some(eval) = &crate_info.evaluation {
            let status_str = common::format_acceptance_status(eval.accepted);
            let colored_status = if use_colors {
                if eval.accepted {
                    status_str.green().bold().to_string()
                } else {
                    status_str.red().bold().to_string()
                }
            } else {
                status_str.to_string()
            };
            writeln!(writer, "Evaluation Result: {colored_status}")?;

            if !eval.reasons.is_empty() {
                writeln!(writer, "Reasons          :")?;
                for reason in &eval.reasons {
                    writeln!(writer, "  - {reason}")?;
                }
            }
        }

        // Build lookup map for quick metric access
        let metric_map: HashMap<&str, &Metric> = crate_info.metrics.iter().map(|m| (m.name(), m)).collect();

        // Use common grouping function to get metric names by category
        let metrics_by_category = common::group_metrics_by_category(&crate_info.metrics);

        // Display metrics grouped by category
        for category in MetricCategory::iter() {
            if let Some(metric_names) = metrics_by_category.get(&category) {
                writeln!(writer)?;
                if use_colors {
                    let category_str = category.to_string();
                    writeln!(writer, "{}", category_str.bold())?;
                } else {
                    writeln!(writer, "{category}")?;
                }

                // Compute max metric name length for alignment
                let max_name_len = metric_names.iter().map(|name| name.len()).max().unwrap_or(0);

                // Get terminal width and calculate available space for values
                let term_width = get_terminal_width();
                // Indent for metric lines: "  " (2) + metric_name + " : " (3)
                let value_indent = 2 + max_name_len + 3;

                for &metric_name in metric_names {
                    if let Some(&metric) = metric_map.get(metric_name) {
                        let formatted_value = metric.value.as_ref().map_or_else(|| "n/a".to_string(), common::format_metric_value);

                        // Wrap the value text
                        let wrapped_lines = wrap_text(&formatted_value, term_width, value_indent);

                        // Write first line with metric name
                        if let Some(first_line) = wrapped_lines.first() {
                            writeln!(writer, "  {:<width$} : {}", metric.name(), first_line, width = max_name_len)?;

                            // Write continuation lines
                            for line in wrapped_lines.iter().skip(1) {
                                writeln!(writer, "{line}")?;
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

/// Get the terminal width, defaulting to 80 if not detectable
fn get_terminal_width() -> usize {
    terminal_size().map_or(80, |(Width(w), _)| w as usize)
}

/// Word-wrap text to fit within a given width, with indentation for continuation lines
fn wrap_text(text: &str, width: usize, indent: usize) -> Vec<String> {
    if width <= indent {
        // Not enough space, return single line
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut is_first_line = true;

    for word in text.split_whitespace() {
        let word_len = word.len();

        // Check if adding this word would exceed the width
        let separator_len = usize::from(!current_line.is_empty()); // space before word
        let line_width = if is_first_line {
            current_line.len()
        } else {
            indent + current_line.len()
        };

        if !current_line.is_empty() && line_width + separator_len + word_len > width {
            // Start a new line
            if is_first_line {
                lines.push(current_line);
                is_first_line = false;
            } else {
                lines.push(format!("{:indent$}{}", "", current_line, indent = indent));
            }
            current_line = word.to_string();
        } else {
            // Add word to current line
            if !current_line.is_empty() {
                current_line.push(' ');
            }
            current_line.push_str(word);
        }
    }

    // Add the last line
    if !current_line.is_empty() {
        if is_first_line {
            lines.push(current_line);
        } else {
            lines.push(format!("{:indent$}{}", "", current_line, indent = indent));
        }
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::EvaluationOutcome;
    use crate::metrics::{MetricDef, MetricValue};

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
    fn test_generate_empty_crates() {
        let crates: Vec<ReportableCrate> = vec![];
        let mut output = String::new();
        let result = generate(&crates, false, &mut output);
        result.unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_generate_single_crate_no_ranking() {
        let crates = vec![create_test_crate("test_crate", "1.0.0", None)];
        let mut output = String::new();
        let result = generate(&crates, false, &mut output);
        result.unwrap();
        // Output should contain crate information but no ranking
        assert!(!output.contains("Evaluation Result"));
        assert!(!output.contains("ACCEPTABLE"));
    }

    #[test]
    fn test_generate_single_crate_with_ranking_accepted() {
        let eval = EvaluationOutcome {
            accepted: true,
            reasons: vec!["Good quality".to_string()],
        };
        let crates = vec![create_test_crate("test_crate", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, false, &mut output);
        result.unwrap();
        assert!(output.contains("Evaluation Result"));
        assert!(output.contains("ACCEPTABLE"));
    }

    #[test]
    fn test_generate_single_crate_with_ranking_denied() {
        let eval = EvaluationOutcome {
            accepted: false,
            reasons: vec!["Security issues".to_string()],
        };
        let crates = vec![create_test_crate("test_crate", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, false, &mut output);
        result.unwrap();
        assert!(output.contains("Evaluation Result"));
        assert!(output.contains("NOT ACCEPTABLE"));
    }

    #[test]
    fn test_generate_multiple_crates() {
        let crates = vec![create_test_crate("zebra", "1.0.0", None), create_test_crate("alpha", "2.0.0", None)];
        let mut output = String::new();
        let result = generate(&crates, false, &mut output);
        result.unwrap();
        // Should have separator between crates
        assert!(output.contains("═══════════════════════════════════════"));
    }

    #[test]
    fn test_generate_color_mode_never() {
        let eval = EvaluationOutcome {
            accepted: true,
            reasons: vec![],
        };
        let crates = vec![create_test_crate("test", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, false, &mut output);
        result.unwrap();
        // Should not contain ANSI color codes
        assert!(!output.contains("\x1b["));
    }

    #[test]
    fn test_wrap_text_short() {
        let text = "short text";
        let lines = wrap_text(text, 80, 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "short text");
    }

    #[test]
    fn test_wrap_text_long() {
        let text = "This is a very long text that should be wrapped at word boundaries when it exceeds the specified width";
        let lines = wrap_text(text, 40, 10);
        assert!(lines.len() > 1);
        // First line should not be indented
        assert!(!lines[0].starts_with(' '));
        // Continuation lines should be indented
        if lines.len() > 1 {
            assert!(lines[1].starts_with("          ")); // 10 spaces
        }
    }

    #[test]
    fn test_wrap_text_exact_fit() {
        let text = "word1 word2 word3";
        let lines = wrap_text(text, 17, 5);
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn test_wrap_text_empty() {
        let text = "";
        let lines = wrap_text(text, 80, 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0], "");
    }
}
