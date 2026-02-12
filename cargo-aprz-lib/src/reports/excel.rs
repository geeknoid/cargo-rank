use super::{ReportableCrate, common};
use crate::Result;
use crate::expr::{Appraisal, Risk};
use crate::metrics::{MetricCategory, MetricValue};
use rust_xlsxwriter::{Color, DocProperties, Format, FormatAlign, Workbook};
use std::io::Write;
use strum::IntoEnumIterator;

#[expect(unused_results, reason = "rust_xlsxwriter methods return &mut Worksheet for chaining")]
pub fn generate<W: Write>(crates: &[ReportableCrate], writer: &mut W) -> Result<()> {
    let mut workbook = Workbook::new();

    // Set document properties
    let properties = DocProperties::new().set_author("cargo-aprz");
    workbook.set_properties(&properties);

    let worksheet = workbook.add_worksheet().set_name("Crate Metrics")?;

    // Create formats
    let bold_format = Format::new().set_bold();
    let category_format = Format::new()
        .set_bold()
        .set_background_color(Color::RGB(0x00FE_D7AA))
        .set_align(FormatAlign::Left);
    let left_align_format = Format::new().set_align(FormatAlign::Left);
    let low_risk_format = Format::new()
        .set_background_color(Color::RGB(0x00C8_E6C9))
        .set_font_color(Color::RGB(0x002E_7D32))
        .set_bold();
    let medium_risk_format = Format::new()
        .set_background_color(Color::RGB(0x00FF_F9C4))
        .set_font_color(Color::RGB(0x00F5_7F17))
        .set_bold();
    let high_risk_format = Format::new()
        .set_background_color(Color::RGB(0x00FF_CDD2))
        .set_font_color(Color::RGB(0x00C6_2828))
        .set_bold();

    // Write crate names as column headers (starting from column B)
    for (col_idx, crate_info) in crates.iter().enumerate() {
        let header = format!("{} v{}", crate_info.name, crate_info.version);
        #[expect(clippy::cast_possible_truncation, reason = "Column index limited by Excel's u16 column limit")]
        worksheet.write_string_with_format(0, (col_idx + 1) as u16, &header, &bold_format)?;
    }

    // Freeze the first column (metric names) and first row (headers)
    worksheet.set_freeze_panes(1, 1)?;

    // Build per-crate metric lookup maps for O(1) access in the inner loop
    let crate_metric_maps = common::build_metric_lookup_maps(crates);

    // Group metrics by category across all crates
    let metrics_by_category = common::group_all_metrics_by_category(crates.iter().map(|c| c.metrics.as_slice()));

    // Write metrics as rows, grouped by category
    let mut row = 1;

    // Add appraisal rows if any crate has one
    let has_appraisals = crates.iter().any(|c| c.appraisal.is_some());
    if has_appraisals {
        // Result row with colored cells
        worksheet.write_string_with_format(row, 0, "Appraisals", &bold_format)?;
        for (col_idx, crate_info) in crates.iter().enumerate() {
            if let Some(eval) = &crate_info.appraisal {
                let value = common::format_appraisal_status(eval);
                let format = match eval.risk {
                    Risk::Low => &low_risk_format,
                    Risk::Medium => &medium_risk_format,
                    Risk::High => &high_risk_format,
                };
                #[expect(clippy::cast_possible_truncation, reason = "Column index limited by Excel's u16 column limit")]
                worksheet.write_string_with_format(row, (col_idx + 1) as u16, value, format)?;
            }
        }
        row += 1;

        // Reasons row
        worksheet.write_string_with_format(row, 0, "Reasons", &bold_format)?;
        write_eval_row(worksheet, row, crates, |eval| common::join_with(
            eval.expression_outcomes.iter().map(common::outcome_icon_name), "; "))?;
        row += 1;

        // Add blank row after evaluation
        row += 1;
    }

    // Write metrics grouped by category
    for category in MetricCategory::iter() {
        if let Some(category_metric_names) = metrics_by_category.get(&category) {
            // Write category header (uppercase and bold with background color)
            worksheet.write_string_with_format(row, 0, category.as_uppercase_str(), &category_format)?;

            // Fill the rest of the category row with the same background color
            #[expect(clippy::cast_possible_truncation, reason = "Column count is limited by Excel's u16 column limit")]
            for c in 1..=crates.len() as u16 {
                worksheet.write_blank(row, c, &category_format)?;
            }

            row += 1;

            // Write each metric in this category
            for &metric_name in category_metric_names {
                worksheet.write_string(row, 0, metric_name)?;

                // Write values for each crate
                for (col_idx, metric_map) in crate_metric_maps.iter().enumerate() {
                    if let Some(metric) = metric_map.get(metric_name)
                        && let Some(ref value) = metric.value
                    {
                        #[expect(clippy::cast_possible_truncation, reason = "Column index limited by Excel's u16 column limit")]
                        write_metric_value(worksheet, row, (col_idx + 1) as u16, metric_name, value, &left_align_format)?;
                    }
                }
                row += 1;
            }

            // Add blank row after category
            row += 1;
        }
    }

    // Auto-fit all columns
    worksheet.autofit();

    // Write workbook to output
    let data = workbook.save_to_buffer()?;
    writer.write_all(&data)?;

    Ok(())
}

#[expect(unused_results, reason = "rust_xlsxwriter methods return &mut Worksheet for chaining")]
#[expect(clippy::cast_precision_loss, reason = "Intentional conversion to f64 for Excel output")]
fn write_metric_value(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    metric_name: &str,
    value: &MetricValue,
    format: &Format,
) -> Result<()> {
    match value {
        MetricValue::UInt(u) => {
            worksheet.write_number_with_format(row, col, *u as f64, format)?;
        }
        MetricValue::Float(f) => {
            worksheet.write_number_with_format(row, col, *f, format)?;
        }
        MetricValue::Boolean(b) => {
            worksheet.write_boolean_with_format(row, col, *b, format)?;
        }
        MetricValue::String(s) => {
            // Check if this is a URL
            if common::is_url(s.as_str()) {
                worksheet.write_url(row, col, s.as_str())?;
            }
            // Check if this is keywords or categories
            else if common::is_keywords_metric(metric_name) || common::is_categories_metric(metric_name) {
                // For keywords/categories, format with # prefix
                let formatted = common::format_keywords_or_categories_with_prefix(s.as_str());
                worksheet.write_string_with_format(row, col, formatted, format)?;
            } else {
                worksheet.write_string_with_format(row, col, s.as_str(), format)?;
            }
        }
        MetricValue::DateTime(dt) => {
            worksheet.write_string_with_format(row, col, dt.format("%Y-%m-%d").to_string(), format)?;
        }
        MetricValue::List(_) => {
            // Format list as comma-separated string
            let formatted = common::format_metric_value(value);
            worksheet.write_string_with_format(row, col, formatted, format)?;
        }
    }
    Ok(())
}

/// Helper function to write a evaluation row (Status or Reasons)
#[expect(unused_results, reason = "rust_xlsxwriter methods return &mut Worksheet for chaining")]
fn write_eval_row<F>(worksheet: &mut rust_xlsxwriter::Worksheet, row: u32, crates: &[ReportableCrate], extract_value: F) -> Result<()>
where
    F: Fn(&Appraisal) -> String,
{
    for (col_idx, crate_info) in crates.iter().enumerate() {
        if let Some(eval) = &crate_info.appraisal {
            let value = extract_value(eval);
            #[expect(clippy::cast_possible_truncation, reason = "Column index limited by Excel's u16 column limit")]
            worksheet.write_string(row, (col_idx + 1) as u16, &value)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{ExpressionDisposition, ExpressionOutcome};
    use crate::metrics::{Metric, MetricDef};
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
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime (rust_xlsxwriter)")]
    fn test_generate_empty_crates() {
        let crates: Vec<ReportableCrate> = vec![];
        let mut output = Vec::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        // Should generate a valid Excel file (has content)
        assert!(!output.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime (rust_xlsxwriter)")]
    fn test_generate_single_crate_no_evaluation() {
        let crates = vec![create_test_crate("test_crate", "1.2.3", None)];
        let mut output = Vec::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        // Should generate a valid Excel file
        assert!(!output.is_empty());
        // Excel files start with PK (ZIP signature)
        assert_eq!(&output[0..2], b"PK");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime (rust_xlsxwriter)")]
    fn test_generate_single_crate_with_evaluation() {
        let eval = Appraisal {
            risk: Risk::Low,
            expression_outcomes: vec![ExpressionOutcome::new("good".into(), "Good".into(), ExpressionDisposition::True)],
            available_points: 1,
            awarded_points: 1,
            score: 100.0,
        };
        let crates = vec![create_test_crate("test_crate", "1.0.0", Some(eval))];
        let mut output = Vec::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        assert!(!output.is_empty());
        assert_eq!(&output[0..2], b"PK");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime (rust_xlsxwriter)")]
    fn test_generate_multiple_crates() {
        let crates = vec![
            create_test_crate("crate_a", "1.0.0", None),
            create_test_crate("crate_b", "2.0.0", None),
        ];
        let mut output = Vec::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        assert!(!output.is_empty());
        // Verify it's a valid ZIP/Excel file
        assert_eq!(&output[0..2], b"PK");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime (rust_xlsxwriter)")]
    fn test_generate_denied_status() {
        let eval = Appraisal {
            risk: Risk::High,
            expression_outcomes: vec![ExpressionOutcome::new("security".into(), "Security issue".into(), ExpressionDisposition::False)],
            available_points: 1,
            awarded_points: 0,
            score: 0.0,
        };
        let crates = vec![create_test_crate("bad_crate", "1.0.0", Some(eval))];
        let mut output = Vec::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        assert!(!output.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime (rust_xlsxwriter)")]
    fn test_generate_with_missing_data() {
        let crates = vec![create_test_crate("missing", "1.0.0", None)];
        let mut output = Vec::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        // Should still generate valid file even with missing data
        assert!(!output.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime (rust_xlsxwriter)")]
    fn test_generate_mixed_found_and_missing() {
        let crates = vec![create_test_crate("good", "1.0.0", None), create_test_crate("bad", "1.0.0", None)];
        let mut output = Vec::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        assert!(!output.is_empty());
    }
}
