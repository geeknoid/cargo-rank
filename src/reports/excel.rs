//! Excel report generation.

use crate::Result;
use crate::config::Config;
use crate::facts::OwnerKind;
use crate::facts::{CrateFacts, ProviderResult};
use crate::ranking::PolicyOutcome;
use crate::ranking::RankingOutcome;
use crate::ranking::extract_reasons;
use rust_xlsxwriter::{Format, Workbook};
use std::io::Write;

/// Generate an Excel report for crate rankings to a writer
///
/// # Errors
///
/// Returns an error if the Excel file cannot be created or written
#[expect(unused_results, reason = "rust_xlsxwriter methods return &mut Worksheet for chaining")]
#[expect(clippy::too_many_lines, reason = "Report generation requires detailed formatting logic")]
pub fn generate<W: Write>(crates: &[(CrateFacts, RankingOutcome)], config: &Config, writer: &mut W) -> Result<()> {
    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    // Headers
    let headers = [
        "Crate group type",
        "Category",
        "Crate name",
        "Crate version",
        "Description",
        "Dependencies",
        "Crate color",
        "Crate score",
        "Notes",
        "Repository",
        "Number of downloads (last month)",
        "Number of downloads (total)",
        "License",
        "Owner count (total)",
        "Owner count (users)",
        "Owner count (teams)",
        "Last version",
        "Last version release date",
        "MSRV for last version",
        "First version",
        "First version release date",
        // ... Add other headers as needed
    ];

    let bold = Format::new().set_bold();
    for (col, header) in headers.iter().enumerate() {
        worksheet.write_string_with_format(0, u16::try_from(col)?, *header, &bold)?;
    }

    for (row, (facts, ranking_data)) in crates.iter().enumerate() {
        let ProviderResult::Found(crate_overall_data) = &facts.crate_overall_data else {
            unreachable!("analyzable crate must have Found data");
        };
        let ProviderResult::Found(crate_version_data) = &facts.crate_version_data else {
            unreachable!("analyzable crate must have Found data");
        };

        let r = u32::try_from(row + 1)?;
        worksheet.write_string(r, 0, ranking_data.dependency_type.to_string())?;
        worksheet.write_string(
            r,
            1,
            if crate_overall_data.categories.is_empty() {
                String::new()
            } else {
                crate_overall_data.categories.join(",")
            },
        )?;
        worksheet.write_string(r, 2, &crate_overall_data.name)?;
        worksheet.write_string(r, 3, crate_version_data.version.to_string())?;
        worksheet.write_string(r, 4, crate_version_data.description.as_deref().unwrap_or(""))?;
        worksheet.write_string(r, 5, "")?; // Dependencies field removed
        let color_name = match config.color_index_for_score(ranking_data.overall_score) {
            Some(2) => "Excellent",
            Some(1) => "Good",
            Some(0) => "Bad",
            _ => "N/A",
        };
        worksheet.write_string(r, 6, color_name)?;
        worksheet.write_number(r, 7, ranking_data.overall_score)?;
        let reasons = extract_reasons(&ranking_data.details);
        worksheet.write_string(r, 8, reasons.join(", "))?;
        worksheet.write_string(r, 9, crate_overall_data.repository.as_ref().map_or("", |u| u.as_str()))?;
        #[expect(clippy::cast_precision_loss, reason = "downloads fit in f64")]
        worksheet.write_number(
            r,
            10,
            crate_overall_data.monthly_downloads.last().map_or(0, |(_, downloads)| *downloads) as f64,
        )?;
        #[expect(clippy::cast_precision_loss, reason = "downloads fit in f64")]
        worksheet.write_number(r, 11, crate_overall_data.downloads as f64)?;
        worksheet.write_string(r, 12, crate_version_data.license.as_deref().unwrap_or("unknown"))?;

        let user_count = crate_overall_data.owners.iter().filter(|x| x.kind == OwnerKind::User).count();
        let team_count = crate_overall_data.owners.iter().filter(|x| x.kind == OwnerKind::Team).count();

        #[expect(clippy::cast_precision_loss, reason = "owner count fits in f64")]
        worksheet.write_number(r, 13, (user_count + team_count) as f64)?;
        #[expect(clippy::cast_precision_loss, reason = "owner count fits in f64")]
        worksheet.write_number(r, 14, user_count as f64)?;
        #[expect(clippy::cast_precision_loss, reason = "owner count fits in f64")]
        worksheet.write_number(r, 15, team_count as f64)?;

        // Current version information
        worksheet.write_string(r, 16, crate_version_data.version.to_string())?;
        worksheet.write_string(r, 17, crate_version_data.created_at.to_string())?;
        worksheet.write_string(r, 18, crate_version_data.rust_version.as_deref().unwrap_or("unknown"))?;

        // Leave columns 19-20 empty (previously used for "first version" which we no longer track)
        worksheet.write_string(r, 19, "")?;
        worksheet.write_string(r, 20, "")?;
    }

    // Create a second worksheet with detailed metric breakdown
    let details_sheet = workbook.add_worksheet();
    details_sheet.set_name("Detailed Rules")?;

    // Headers for detailed metrics
    let metric_headers = [
        "Crate name",
        "Crate version",
        "Category",
        "Check description",
        "Passed",
        "Points received",
    ];

    for (col, header) in metric_headers.iter().enumerate() {
        details_sheet.write_string_with_format(0, u16::try_from(col)?, *header, &bold)?;
    }

    let mut detail_row = 1u32;
    for (details, ranking_data) in crates {
        let ProviderResult::Found(crate_overall_data) = &details.crate_overall_data else {
            unreachable!("analyzable crate must have Found data");
        };
        let ProviderResult::Found(crate_version_data) = &details.crate_version_data else {
            unreachable!("analyzable crate must have Found data");
        };

        for outcome in ranking_data.details.values() {
            let (passed, points, description) = match outcome {
                PolicyOutcome::Match(points, info) => (true, *points, info.as_str()),
                PolicyOutcome::NoMatch(reason) => (false, 0.0, reason.as_str()),
            };

            details_sheet.write_string(detail_row, 0, &crate_overall_data.name)?;
            details_sheet.write_string(detail_row, 1, crate_version_data.version.to_string())?;
            details_sheet.write_string(detail_row, 2, "")?; // Category removed
            details_sheet.write_string(detail_row, 3, description)?;
            details_sheet.write_string(detail_row, 4, if passed { "Yes" } else { "No" })?;
            details_sheet.write_number(detail_row, 5, points)?;
            detail_row += 1;
        }
    }

    let buffer = workbook.save_to_buffer()?;
    writer.write_all(&buffer)?;
    Ok(())
}
