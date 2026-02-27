use super::{ReportableCrate, common};
use crate::Result;
use crate::expr::{ExpressionDisposition, Risk};
use crate::metrics::MetricCategory;
use chrono::{DateTime, Local};
use core::fmt::Write;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use strum::IntoEnumIterator;

const FERRIS_FAVICON: &str = "data:image/svg+xml,%3Csvg viewBox='0 0 1200 800' xmlns='http://www.w3.org/2000/svg'%3E%3Cg%3E%3Cg transform='matrix(1,0,0,1,654.172,668.359)'%3E%3Cpath d='M0,-322.648C-114.597,-322.648 -218.172,-308.869 -296.172,-286.419L-296.172,-291.49C-374.172,-266.395 -423.853,-231.531 -423.853,-192.984C-423.853,-186.907 -422.508,-180.922 -420.15,-175.053L-428.134,-160.732C-428.134,-160.732 -434.547,-152.373 -423.199,-134.733C-413.189,-119.179 -363.035,-58.295 -336.571,-26.413C-325.204,-10.065 -317.488,0 -316.814,-0.973C-315.753,-2.516 -323.878,-33.202 -346.453,-68.215C-356.986,-87.02 -369.811,-111.934 -377.361,-130.335C-356.28,-116.993 -328.172,-104.89 -296.172,-94.474L-296.172,-94.633C-218.172,-72.18 -114.597,-58.404 0,-58.404C131.156,-58.404 248.828,-76.45 327.828,-104.895L327.828,-276.153C248.828,-304.6 131.156,-322.648 0,-322.648' fill='%23a52b00'/%3E%3C/g%3E%3Cg transform='matrix(1,0,0,1,1177.87,277.21)'%3E%3Cpath d='M0,227.175L-88.296,162.132C-89.126,159.237 -89.956,156.345 -90.812,153.474L-61.81,111.458C-58.849,107.184 -58.252,101.629 -60.175,96.755C-62.1,91.905 -66.311,88.428 -71.292,87.576L-120.335,79.255C-122.233,75.376 -124.225,71.557 -126.224,67.771L-105.62,20.599C-103.501,15.793 -103.947,10.209 -106.759,5.848C-109.556,1.465 -114.31,-1.094 -119.376,-0.895L-169.146,0.914C-171.723,-2.442 -174.34,-5.766 -177.012,-9.032L-165.574,-59.592C-164.415,-64.724 -165.876,-70.1 -169.453,-73.83C-173.008,-77.546 -178.175,-79.084 -183.089,-77.88L-231.567,-65.961C-234.707,-68.736 -237.897,-71.474 -241.126,-74.157L-239.381,-126.064C-239.193,-131.318 -241.643,-136.311 -245.849,-139.227C-250.053,-142.161 -255.389,-142.603 -259.987,-140.423L-305.213,-118.921C-308.853,-121.011 -312.515,-123.081 -316.218,-125.084L-324.209,-176.232C-325.021,-181.413 -328.355,-185.816 -333.024,-187.826C-337.679,-189.848 -343.014,-189.193 -347.101,-186.116L-387.422,-155.863C-391.392,-157.181 -395.38,-158.446 -399.418,-159.655L-416.798,-208.159C-418.564,-213.104 -422.64,-216.735 -427.608,-217.756C-432.561,-218.768 -437.656,-217.053 -441.091,-213.217L-475.029,-175.246C-479.133,-175.717 -483.239,-176.147 -487.356,-176.505L-513.564,-220.659C-516.22,-225.131 -520.908,-227.852 -525.961,-227.852C-531.002,-227.852 -535.7,-225.131 -538.333,-220.659L-564.547,-176.505C-568.666,-176.147 -572.791,-175.717 -576.888,-175.246L-610.831,-213.217C-614.268,-217.053 -619.382,-218.768 -624.318,-217.756C-629.284,-216.721 -633.363,-213.104 -635.124,-208.159L-652.517,-159.655C-656.544,-158.446 -660.534,-157.173 -664.514,-155.863L-704.822,-186.116C-708.92,-189.204 -714.254,-189.857 -718.92,-187.826C-723.57,-185.816 -726.917,-181.413 -727.723,-176.232L-735.72,-125.084C-739.42,-123.081 -743.083,-121.022 -746.734,-118.921L-791.956,-140.423C-796.548,-142.612 -801.908,-142.161 -806.091,-139.227C-810.292,-136.311 -812.747,-131.318 -812.557,-126.064L-810.821,-74.157C-814.04,-71.474 -817.224,-68.736 -820.379,-65.961L-868.849,-77.88C-873.774,-79.075 -878.935,-77.546 -882.499,-73.83C-886.084,-70.1 -887.538,-64.724 -886.384,-59.592L-874.969,-9.032C-877.618,-5.753 -880.239,-2.442 -882.808,0.914L-932.579,-0.895C-937.602,-1.043 -942.396,1.465 -945.202,5.848C-948.014,10.209 -948.439,15.793 -946.348,20.599L-925.729,67.771C-927.732,71.557 -929.721,75.376 -931.635,79.255L-980.675,87.576C-985.657,88.417 -989.858,91.892 -991.795,96.755C-993.72,101.629 -993.095,107.184 -990.156,111.458L-961.146,153.474C-961.37,154.215 -961.576,154.964 -961.799,155.707L-1043.82,242.829C-1043.82,242.829 -1056.38,252.68 -1038.09,275.831C-1021.95,296.252 -939.097,377.207 -895.338,419.62C-876.855,441.152 -864.195,454.486 -862.872,453.332C-860.784,451.5 -871.743,412.326 -908.147,366.362C-936.207,325.123 -972.625,261.696 -964.086,254.385C-964.086,254.385 -954.372,242.054 -934.882,233.178C-934.169,233.749 -935.619,232.613 -934.882,233.178C-934.882,233.178 -523.568,422.914 -142.036,236.388C-98.452,228.571 -72.068,251.917 -72.068,251.917C-62.969,257.193 -86.531,322.412 -105.906,365.583C-132.259,414.606 -136.123,452.859 -133.888,454.185C-132.479,455.027 -122.89,440.438 -109.214,417.219C-75.469,370.196 -11.675,280.554 0,258.781C13.239,234.094 0,227.175 0,227.175' fill='%23f74c00'/%3E%3C/g%3E%3C/g%3E%3C/svg%3E";

pub fn generate<W: Write>(crates: &[ReportableCrate], timestamp: DateTime<Local>, writer: &mut W) -> Result<()> {
    let has_appraisals = crates.iter().any(|c| c.appraisal.is_some());

    // Compute summary statistics
    let total = crates.len();
    let low_count = crates.iter().filter(|c| c.appraisal.as_ref().is_some_and(|a| a.risk == Risk::Low)).count();
    let medium_count = crates.iter().filter(|c| c.appraisal.as_ref().is_some_and(|a| a.risk == Risk::Medium)).count();
    let high_count = crates.iter().filter(|c| c.appraisal.as_ref().is_some_and(|a| a.risk == Risk::High)).count();
    let not_evaluated_count = crates.iter().filter(|c| c.appraisal.is_none()).count();
    let high_risk_crates: Vec<(&str, String)> = crates
        .iter()
        .filter(|c| c.appraisal.as_ref().is_some_and(|a| a.risk == Risk::High))
        .map(|c| (c.name.as_ref(), c.version.to_string()))
        .collect();
    let medium_risk_crates: Vec<(&str, String)> = crates
        .iter()
        .filter(|c| c.appraisal.as_ref().is_some_and(|a| a.risk == Risk::Medium))
        .map(|c| (c.name.as_ref(), c.version.to_string()))
        .collect();

    writeln!(writer, "<!DOCTYPE html>")?;
    writeln!(writer, "<html>")?;
    writeln!(writer, "<head>")?;
    writeln!(writer, "  <meta charset=\"UTF-8\">")?;
    writeln!(writer, "  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">")?;
    writeln!(writer, "  <title>Crate Appraisal Report</title>")?;
    writeln!(writer, "  <link rel=\"icon\" type=\"image/svg+xml\" href=\"{FERRIS_FAVICON}\">")?;
    write_styles(writer)?;
    writeln!(writer, "</head>")?;
    writeln!(writer, "<body>")?;

    write_header(writer, timestamp)?;

    // Summary section
    if has_appraisals {
        write_summary(writer, total, low_count, medium_count, high_count, not_evaluated_count, &high_risk_crates, &medium_risk_crates)?;
        write_filter_bar(writer)?;
    }

    // Crate cards
    let metrics_by_category = common::group_all_metrics_by_category(crates.iter().map(|c| c.metrics.as_slice()));
    let crate_metric_maps = common::build_metric_lookup_maps(crates);
    writeln!(writer, "  <div id=\"crate-list\">")?;
    for (crate_index, crate_info) in crates.iter().enumerate() {
        let risk_attr = crate_info.appraisal.as_ref().map_or("none", |a| match a.risk {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
        });
        let anchor_id = crate_anchor_id(&crate_info.name, &crate_info.version.to_string());
        writeln!(writer, "    <div class=\"crate-card\" id=\"{anchor_id}\" data-risk=\"{risk_attr}\">")?;
        write_crate_card_header(writer, crate_info)?;

        // Collect which tabs this crate has
        let has_appraisal_tab = crate_info.appraisal.as_ref().is_some_and(|a| !a.expression_outcomes.is_empty());
        let metric_map = &crate_metric_maps[crate_index];
        let mut crate_categories: Vec<MetricCategory> = Vec::new();
        for category in MetricCategory::iter() {
            if metrics_by_category.get(&category).is_some_and(|cm| cm.iter().any(|&name| metric_map.contains_key(name))) {
                crate_categories.push(category);
            }
        }

        // Tab navigation
        let card_id = format!("card-{crate_index}");
        writeln!(writer, "      <div class=\"tabs\">")?;
        writeln!(writer, "        <div class=\"tab-nav\">")?;
        let mut tab_index = 0u32;
        if has_appraisal_tab {
            writeln!(writer, "          <button class=\"tab-btn active\" data-tab=\"{card_id}-appraisal\" onclick=\"switchTab(this)\">Appraisal</button>")?;
            tab_index += 1;
        }
        for cat in &crate_categories {
            let active = if tab_index == 0 { " active" } else { "" };
            writeln!(writer, "          <button class=\"tab-btn{active}\" data-tab=\"{card_id}-{cat}\" onclick=\"switchTab(this)\">{cat}</button>")?;
            tab_index += 1;
        }
        writeln!(writer, "        </div>")?;

        // Tab panels
        if let Some(appraisal) = crate_info.appraisal.as_ref().filter(|a| !a.expression_outcomes.is_empty()) {
            writeln!(writer, "        <div class=\"tab-panel active\" id=\"{card_id}-appraisal\">")?;
            write_appraisal_table(writer, appraisal)?;
            writeln!(writer, "        </div>")?;
        }

        let mut panel_index = u32::from(has_appraisal_tab);
        for category in &crate_categories {
            let active = if panel_index == 0 { " active" } else { "" };
            writeln!(writer, "        <div class=\"tab-panel{active}\" id=\"{card_id}-{category}\">")?;
            write_metrics_category(writer, *category, &metrics_by_category, metric_map)?;
            writeln!(writer, "        </div>")?;
            panel_index += 1;
        }

        writeln!(writer, "      </div>")?;

        writeln!(writer, "    </div>")?;
    }
    writeln!(writer, "  </div>")?;

    write_scripts(writer, has_appraisals)?;
    writeln!(writer, "</body>")?;
    writeln!(writer, "</html>")?;

    Ok(())
}

#[expect(clippy::too_many_lines, reason = "CSS template generation naturally requires many lines")]
fn write_styles<W: Write>(writer: &mut W) -> Result<()> {
    writeln!(writer, "  <style>")?;
    // CSS custom properties
    writeln!(writer, "    :root {{")?;
    writeln!(writer, "      --bg-color: #f0f2f5;")?;
    writeln!(writer, "      --card-bg: #ffffff;")?;
    writeln!(writer, "      --text-color: #1a202c;")?;
    writeln!(writer, "      --text-secondary: #64748b;")?;
    writeln!(writer, "      --border-color: #e2e8f0;")?;
    writeln!(writer, "      --category-bg: #fef3e2;")?;
    writeln!(writer, "      --category-text: #9a3412;")?;
    writeln!(writer, "      --hover-bg: #f8fafc;")?;
    writeln!(writer, "      --accent-color: #3b82f6;")?;
    writeln!(writer, "      --shadow: 0 1px 3px rgba(0,0,0,0.08), 0 4px 16px rgba(0,0,0,0.04);")?;
    writeln!(writer, "      --summary-card-bg: #ffffff;")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    @media (prefers-color-scheme: dark) {{")?;
    writeln!(writer, "      :root {{")?;
    writeln!(writer, "        --bg-color: #0f172a;")?;
    writeln!(writer, "        --card-bg: #1e293b;")?;
    writeln!(writer, "        --text-color: #e2e8f0;")?;
    writeln!(writer, "        --text-secondary: #94a3b8;")?;
    writeln!(writer, "        --border-color: #334155;")?;
    writeln!(writer, "        --category-bg: #451a03;")?;
    writeln!(writer, "        --category-text: #fdba74;")?;
    writeln!(writer, "        --hover-bg: #263044;")?;
    writeln!(writer, "        --accent-color: #60a5fa;")?;
    writeln!(writer, "        --shadow: 0 1px 3px rgba(0,0,0,0.3), 0 4px 16px rgba(0,0,0,0.2);")?;
    writeln!(writer, "        --summary-card-bg: #1e293b;")?;
    writeln!(writer, "      }}")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    body.dark-theme {{")?;
    writeln!(writer, "      --bg-color: #0f172a; --card-bg: #1e293b; --text-color: #e2e8f0; --text-secondary: #94a3b8;")?;
    writeln!(writer, "      --border-color: #334155; --category-bg: #451a03; --category-text: #fdba74;")?;
    writeln!(writer, "      --hover-bg: #263044; --accent-color: #60a5fa;")?;
    writeln!(writer, "      --shadow: 0 1px 3px rgba(0,0,0,0.3), 0 4px 16px rgba(0,0,0,0.2); --summary-card-bg: #1e293b;")?;
    writeln!(writer, "      color-scheme: dark;")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    body.light-theme {{")?;
    writeln!(writer, "      --bg-color: #f0f2f5; --card-bg: #ffffff; --text-color: #1a202c; --text-secondary: #64748b;")?;
    writeln!(writer, "      --border-color: #e2e8f0; --category-bg: #fef3e2; --category-text: #9a3412;")?;
    writeln!(writer, "      --hover-bg: #f8fafc; --accent-color: #3b82f6;")?;
    writeln!(writer, "      --shadow: 0 1px 3px rgba(0,0,0,0.08), 0 4px 16px rgba(0,0,0,0.04); --summary-card-bg: #ffffff;")?;
    writeln!(writer, "      color-scheme: light;")?;
    writeln!(writer, "    }}")?;

    // Base styles
    writeln!(writer, "    * {{ box-sizing: border-box; }}")?;
    writeln!(writer, "    body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif; margin: 0; padding: 32px; background: var(--bg-color); color: var(--text-color); transition: background-color 0.3s ease, color 0.3s ease; line-height: 1.5; }}")?;

    // Header
    writeln!(writer, "    .header {{ display: flex; align-items: center; gap: 16px; margin-bottom: 28px; }}")?;
    writeln!(writer, "    .header-content {{ flex: 1; }}")?;
    writeln!(writer, "    h1 {{ margin: 0 0 2px 0; font-size: 26px; font-weight: 700; letter-spacing: -0.5px; }}")?;
    writeln!(writer, "    .subtitle {{ margin: 0; font-size: 13px; color: var(--text-secondary); }}")?;
    writeln!(writer, "    .ferris {{ width: 52px; height: 35px; flex-shrink: 0; }}")?;
    writeln!(writer, "    .theme-toggle {{ background: none; border: 2px solid var(--border-color); border-radius: 8px; width: 40px; height: 40px; cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all 0.2s ease; flex-shrink: 0; }}")?;
    writeln!(writer, "    .theme-toggle:hover {{ border-color: var(--accent-color); }}")?;
    writeln!(writer, "    .theme-toggle svg {{ width: 18px; height: 18px; fill: var(--text-color); opacity: 0.7; }}")?;

    // Summary cards
    writeln!(writer, "    .summary {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: 12px; margin-bottom: 20px; }}")?;
    writeln!(writer, "    .summary-card {{ background: var(--summary-card-bg); border-radius: 10px; padding: 16px 20px; box-shadow: var(--shadow); border: 1px solid var(--border-color); text-align: center; }}")?;
    writeln!(writer, "    .summary-card .label {{ font-size: 11px; text-transform: uppercase; letter-spacing: 0.8px; color: var(--text-secondary); font-weight: 600; margin-bottom: 4px; }}")?;
    writeln!(writer, "    .summary-card .value {{ font-size: 28px; font-weight: 700; }}")?;
    writeln!(writer, "    .summary-card.total .value {{ color: var(--accent-color); }}")?;
    writeln!(writer, "    .summary-card.low .value {{ color: #16a34a; }}")?;
    writeln!(writer, "    .summary-card.medium .value {{ color: #d97706; }}")?;
    writeln!(writer, "    .summary-card.high .value {{ color: #dc2626; }}")?;
    writeln!(writer, "    .summary-card.not-eval .value {{ color: var(--text-secondary); }}")?;

    // Filter bar
    writeln!(writer, "    .filter-bar {{ display: flex; align-items: center; gap: 8px; margin-bottom: 24px; flex-wrap: wrap; }}")?;
    writeln!(writer, "    .filter-bar .label {{ font-size: 13px; font-weight: 600; color: var(--text-secondary); margin-right: 4px; }}")?;
    writeln!(writer, "    .filter-btn {{ border: 1px solid var(--border-color); background: var(--card-bg); color: var(--text-color); padding: 6px 16px; border-radius: 20px; font-size: 13px; cursor: pointer; font-weight: 500; transition: all 0.15s ease; }}")?;
    writeln!(writer, "    .filter-btn:hover {{ border-color: var(--accent-color); }}")?;
    writeln!(writer, "    .filter-btn.active {{ background: var(--accent-color); color: #ffffff; border-color: var(--accent-color); }}")?;

    // Crate card
    writeln!(writer, "    .crate-card {{ background: var(--card-bg); border-radius: 12px; box-shadow: var(--shadow); border: 1px solid var(--border-color); margin-bottom: 20px; overflow: hidden; transition: box-shadow 0.2s ease; }}")?;
    writeln!(writer, "    .crate-card:hover {{ box-shadow: 0 2px 8px rgba(0,0,0,0.12), 0 8px 24px rgba(0,0,0,0.08); }}")?;
    writeln!(writer, "    .crate-card-header {{ display: flex; align-items: center; gap: 12px; padding: 16px 20px; border-bottom: 1px solid var(--border-color); }}")?;
    writeln!(writer, "    .crate-card-header .crate-title {{ font-size: 18px; font-weight: 700; }}")?;
    writeln!(writer, "    .crate-card-header .spacer {{ flex: 1; }}")?;
    writeln!(writer, "    .crate-card-header .header-right {{ display: flex; align-items: center; gap: 12px; }}")?;
    writeln!(writer, "    .crate-card-header.risk-low {{ background: linear-gradient(135deg, #dcfce7 0%, var(--card-bg) 100%); }}")?;
    writeln!(writer, "    .crate-card-header.risk-medium {{ background: linear-gradient(135deg, #fed7aa 0%, var(--card-bg) 100%); }}")?;
    writeln!(writer, "    .crate-card-header.risk-high {{ background: linear-gradient(135deg, #fee2e2 0%, var(--card-bg) 100%); }}")?;
    writeln!(writer, "    body.dark-theme .crate-card-header.risk-low {{ background: linear-gradient(135deg, #14532d 0%, var(--card-bg) 100%); }}")?;
    writeln!(writer, "    body.dark-theme .crate-card-header.risk-medium {{ background: linear-gradient(135deg, #9a3412 0%, var(--card-bg) 100%); }}")?;
    writeln!(writer, "    body.dark-theme .crate-card-header.risk-high {{ background: linear-gradient(135deg, #7f1d1d 0%, var(--card-bg) 100%); }}")?;

    // Risk badges
    writeln!(writer, "    .risk-badge {{ display: inline-block; padding: 4px 12px; border-radius: 16px; font-size: 12px; font-weight: 700; letter-spacing: 0.5px; text-transform: uppercase; white-space: nowrap; }}")?;
    writeln!(writer, "    .risk-badge.low {{ background: #dcfce7; color: #166534; }}")?;
    writeln!(writer, "    .risk-badge.medium {{ background: #fed7aa; color: #9a3412; }}")?;
    writeln!(writer, "    .risk-badge.high {{ background: #fee2e2; color: #991b1b; }}")?;
    writeln!(writer, "    .risk-badge.not-evaluated {{ background: #f1f5f9; color: #64748b; }}")?;
    writeln!(writer, "    body.dark-theme .risk-badge.low {{ background: #14532d; color: #86efac; }}")?;
    writeln!(writer, "    body.dark-theme .risk-badge.medium {{ background: #9a3412; color: #fdba74; }}")?;
    writeln!(writer, "    body.dark-theme .risk-badge.high {{ background: #7f1d1d; color: #fca5a5; }}")?;
    writeln!(writer, "    body.dark-theme .risk-badge.not-evaluated {{ background: #334155; color: #94a3b8; }}")?;

    // Appraisal score
    writeln!(writer, "    .appraisal-score {{ font-size: 14px; font-weight: 600; white-space: nowrap; color: var(--text-secondary); }}")?;

    // Tables within cards
    writeln!(writer, "    .card-section {{ padding: 0; }}")?;
    writeln!(writer, "    .card-section-title {{ font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.8px; color: var(--category-text); background: var(--category-bg); padding: 8px 20px; }}")?;
    writeln!(writer, "    table {{ border-collapse: collapse; width: 100%; }}")?;
    writeln!(writer, "    th {{ text-align: left; padding: 8px 20px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px; color: var(--text-secondary); border-bottom: 2px solid var(--border-color); background: var(--card-bg); }}")?;
    writeln!(writer, "    td {{ padding: 8px 20px; font-size: 14px; border-bottom: 1px solid var(--border-color); vertical-align: top; }}")?;
    writeln!(writer, "    tr:last-child td {{ border-bottom: none; }}")?;
    writeln!(writer, "    tr:hover td {{ background: var(--hover-bg); }}")?;
    writeln!(writer, "    td:first-child {{ font-weight: 500; color: var(--text-secondary); white-space: nowrap; width: 1%; }}")?;

    // Disposition badges
    writeln!(writer, "    .disposition {{ display: inline-block; padding: 2px 10px; border-radius: 4px; font-size: 12px; font-weight: 600; }}")?;
    writeln!(writer, "    .disposition.passed {{ background: #dcfce7; color: #166534; }}")?;
    writeln!(writer, "    .disposition.failed {{ background: #fee2e2; color: #991b1b; }}")?;
    writeln!(writer, "    .disposition.inconclusive {{ background: #f1f5f9; color: #64748b; }}")?;
    writeln!(writer, "    body.dark-theme .disposition.passed {{ background: #14532d; color: #86efac; }}")?;
    writeln!(writer, "    body.dark-theme .disposition.failed {{ background: #7f1d1d; color: #fca5a5; }}")?;
    writeln!(writer, "    body.dark-theme .disposition.inconclusive {{ background: #334155; color: #94a3b8; }}")?;

    // Tabs
    writeln!(writer, "    .tabs {{ }}")?;
    writeln!(writer, "    .tab-nav {{ display: flex; gap: 0; border-bottom: 2px solid var(--border-color); padding: 0 20px; overflow-x: auto; overflow-y: hidden; }}")?;
    writeln!(writer, "    .tab-btn {{ background: none; border: none; border-bottom: 2px solid transparent; margin-bottom: -2px; padding: 10px 16px; font-size: 13px; font-weight: 600; color: var(--text-secondary); cursor: pointer; white-space: nowrap; transition: all 0.15s ease; }}")?;
    writeln!(writer, "    .tab-btn:hover {{ color: var(--text-color); }}")?;
    writeln!(writer, "    .tab-btn.active {{ color: var(--text-color); border-bottom-color: var(--accent-color); background: var(--hover-bg); border-radius: 6px 6px 0 0; }}")?;
    writeln!(writer, "    .tab-panel {{ display: none; }}")?;
    writeln!(writer, "    .tab-panel.active {{ display: block; }}")?;

    // Risk list (shared by high/medium)
    writeln!(writer, "    .risk-list {{ background: var(--card-bg); border-radius: 10px; padding: 14px 20px; box-shadow: var(--shadow); border: 1px solid var(--border-color); margin-bottom: 12px; }}")?;
    writeln!(writer, "    .risk-list strong {{ font-size: 12px; text-transform: uppercase; letter-spacing: 0.5px; }}")?;
    writeln!(writer, "    .risk-list .crate-names {{ margin-top: 6px; font-size: 14px; }}")?;
    writeln!(writer, "    .risk-list .crate-names a {{ text-decoration: none; }}")?;
    writeln!(writer, "    .risk-list .crate-name {{ display: inline-block; padding: 2px 10px; border-radius: 12px; margin: 2px 4px 2px 0; font-weight: 500; font-size: 13px; cursor: pointer; transition: opacity 0.15s; }}")?;
    writeln!(writer, "    .risk-list .crate-name:hover {{ opacity: 0.8; }}")?;
    writeln!(writer, "    .risk-list.high {{ border-left: 4px solid #dc2626; }}")?;
    writeln!(writer, "    .risk-list.high strong {{ color: #dc2626; }}")?;
    writeln!(writer, "    .risk-list.high .crate-name {{ background: #fef2f2; color: #dc2626; }}")?;
    writeln!(writer, "    .risk-list.medium {{ border-left: 4px solid #d97706; }}")?;
    writeln!(writer, "    .risk-list.medium strong {{ color: #d97706; }}")?;
    writeln!(writer, "    .risk-list.medium .crate-name {{ background: #fefce8; color: #854d0e; }}")?;
    writeln!(writer, "    body.dark-theme .risk-list.high .crate-name {{ background: #450a0a; color: #fca5a5; }}")?;
    writeln!(writer, "    body.dark-theme .risk-list.medium .crate-name {{ background: #451a03; color: #fde047; }}")?;

    // Misc
    writeln!(writer, "    .na {{ color: var(--text-secondary); font-style: italic; font-size: 13px; }}")?;
    writeln!(writer, "    a {{ color: var(--accent-color); text-decoration: none; }}")?;
    writeln!(writer, "    a:hover {{ text-decoration: underline; }}")?;
    writeln!(writer, "    @media (max-width: 640px) {{ body {{ padding: 16px; }} .summary {{ grid-template-columns: repeat(2, 1fr); }} }}")?;
    writeln!(writer, "  </style>")?;
    Ok(())
}

fn write_header<W: Write>(writer: &mut W, timestamp: DateTime<Local>) -> Result<()> {
    let date = timestamp.format("%Y-%m-%d").to_string();
    writeln!(writer, "  <div class=\"header\">")?;
    writeln!(writer, "    <svg class=\"ferris\" viewBox=\"0 0 1200 800\" xmlns=\"http://www.w3.org/2000/svg\">")?;
    writeln!(writer, "      <g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,654.172,668.359)\">")?;
    writeln!(writer, "          <path d=\"M0,-322.648C-114.597,-322.648 -218.172,-308.869 -296.172,-286.419L-296.172,-291.49C-374.172,-266.395 -423.853,-231.531 -423.853,-192.984C-423.853,-186.907 -422.508,-180.922 -420.15,-175.053L-428.134,-160.732C-428.134,-160.732 -434.547,-152.373 -423.199,-134.733C-413.189,-119.179 -363.035,-58.295 -336.571,-26.413C-325.204,-10.065 -317.488,0 -316.814,-0.973C-315.753,-2.516 -323.878,-33.202 -346.453,-68.215C-356.986,-87.02 -369.811,-111.934 -377.361,-130.335C-356.28,-116.993 -328.172,-104.89 -296.172,-94.474L-296.172,-94.633C-218.172,-72.18 -114.597,-58.404 0,-58.404C131.156,-58.404 248.828,-76.45 327.828,-104.895L327.828,-276.153C248.828,-304.6 131.156,-322.648 0,-322.648\" style=\"fill:rgb(165,43,0);fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,1099.87,554.94)\">")?;
    writeln!(writer, "          <path d=\"M0,-50.399L-13.433,-78.227C-13.362,-79.283 -13.309,-80.341 -13.309,-81.402C-13.309,-112.95 -46.114,-142.022 -101.306,-165.303L-101.306,2.499C-75.555,-8.365 -54.661,-20.485 -39.72,-33.538C-44.118,-15.855 -59.157,19.917 -71.148,45.073C-90.855,81.054 -97.993,112.376 -97.077,113.926C-96.493,114.904 -89.77,104.533 -79.855,87.726C-56.783,54.85 -13.063,-7.914 -4.325,-23.901C5.574,-42.024 0,-50.399 0,-50.399\" style=\"fill:rgb(165,43,0);fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,1177.87,277.21)\">")?;
    writeln!(writer, "          <path d=\"M0,227.175L-88.296,162.132C-89.126,159.237 -89.956,156.345 -90.812,153.474L-61.81,111.458C-58.849,107.184 -58.252,101.629 -60.175,96.755C-62.1,91.905 -66.311,88.428 -71.292,87.576L-120.335,79.255C-122.233,75.376 -124.225,71.557 -126.224,67.771L-105.62,20.599C-103.501,15.793 -103.947,10.209 -106.759,5.848C-109.556,1.465 -114.31,-1.094 -119.376,-0.895L-169.146,0.914C-171.723,-2.442 -174.34,-5.766 -177.012,-9.032L-165.574,-59.592C-164.415,-64.724 -165.876,-70.1 -169.453,-73.83C-173.008,-77.546 -178.175,-79.084 -183.089,-77.88L-231.567,-65.961C-234.707,-68.736 -237.897,-71.474 -241.126,-74.157L-239.381,-126.064C-239.193,-131.318 -241.643,-136.311 -245.849,-139.227C-250.053,-142.161 -255.389,-142.603 -259.987,-140.423L-305.213,-118.921C-308.853,-121.011 -312.515,-123.081 -316.218,-125.084L-324.209,-176.232C-325.021,-181.413 -328.355,-185.816 -333.024,-187.826C-337.679,-189.848 -343.014,-189.193 -347.101,-186.116L-387.422,-155.863C-391.392,-157.181 -395.38,-158.446 -399.418,-159.655L-416.798,-208.159C-418.564,-213.104 -422.64,-216.735 -427.608,-217.756C-432.561,-218.768 -437.656,-217.053 -441.091,-213.217L-475.029,-175.246C-479.133,-175.717 -483.239,-176.147 -487.356,-176.505L-513.564,-220.659C-516.22,-225.131 -520.908,-227.852 -525.961,-227.852C-531.002,-227.852 -535.7,-225.131 -538.333,-220.659L-564.547,-176.505C-568.666,-176.147 -572.791,-175.717 -576.888,-175.246L-610.831,-213.217C-614.268,-217.053 -619.382,-218.768 -624.318,-217.756C-629.284,-216.721 -633.363,-213.104 -635.124,-208.159L-652.517,-159.655C-656.544,-158.446 -660.534,-157.173 -664.514,-155.863L-704.822,-186.116C-708.92,-189.204 -714.254,-189.857 -718.92,-187.826C-723.57,-185.816 -726.917,-181.413 -727.723,-176.232L-735.72,-125.084C-739.42,-123.081 -743.083,-121.022 -746.734,-118.921L-791.956,-140.423C-796.548,-142.612 -801.908,-142.161 -806.091,-139.227C-810.292,-136.311 -812.747,-131.318 -812.557,-126.064L-810.821,-74.157C-814.04,-71.474 -817.224,-68.736 -820.379,-65.961L-868.849,-77.88C-873.774,-79.075 -878.935,-77.546 -882.499,-73.83C-886.084,-70.1 -887.538,-64.724 -886.384,-59.592L-874.969,-9.032C-877.618,-5.753 -880.239,-2.442 -882.808,0.914L-932.579,-0.895C-937.602,-1.043 -942.396,1.465 -945.202,5.848C-948.014,10.209 -948.439,15.793 -946.348,20.599L-925.729,67.771C-927.732,71.557 -929.721,75.376 -931.635,79.255L-980.675,87.576C-985.657,88.417 -989.858,91.892 -991.795,96.755C-993.72,101.629 -993.095,107.184 -990.156,111.458L-961.146,153.474C-961.37,154.215 -961.576,154.964 -961.799,155.707L-1043.82,242.829C-1043.82,242.829 -1056.38,252.68 -1038.09,275.831C-1021.95,296.252 -939.097,377.207 -895.338,419.62C-876.855,441.152 -864.195,454.486 -862.872,453.332C-860.784,451.5 -871.743,412.326 -908.147,366.362C-936.207,325.123 -972.625,261.696 -964.086,254.385C-964.086,254.385 -954.372,242.054 -934.882,233.178C-934.169,233.749 -935.619,232.613 -934.882,233.178C-934.882,233.178 -523.568,422.914 -142.036,236.388C-98.452,228.571 -72.068,251.917 -72.068,251.917C-62.969,257.193 -86.531,322.412 -105.906,365.583C-132.259,414.606 -136.123,452.859 -133.888,454.185C-132.479,455.027 -122.89,440.438 -109.214,417.219C-75.469,370.196 -11.675,280.554 0,258.781C13.239,234.094 0,227.175 0,227.175\" style=\"fill:rgb(247,76,0);fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,795.856,464.937)\">")?;
    writeln!(writer, "          <path d=\"M0,159.631C1.575,158.289 2.4,157.492 2.4,157.492L-132.25,144.985C-22.348,0 65.618,116.967 74.988,129.879L74.988,159.631L0,159.631Z\" style=\"fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,278.418,211.791)\">")?;
    writeln!(writer, "          <path d=\"M0,253.04C0,253.04 -111.096,209.79 -129.876,163.242C-129.876,163.242 0.515,59.525 -155.497,-50.644L-159.726,89.773C-159.726,89.773 -205.952,45.179 -203.912,-32.91C-203.912,-32.91 -347.685,36.268 -179.436,158.667C-179.436,158.667 -173.76,224.365 -22.459,303.684L0,253.04Z\" style=\"fill:rgb(247,76,0);fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,729.948,492.523)\">")?;
    writeln!(writer, "          <path d=\"M0,-87.016C0,-87.016 41.104,-132.025 82.21,-87.016C82.21,-87.016 114.507,-27.003 82.21,3C82.21,3 29.36,45.009 0,3C0,3 -35.232,-30.006 0,-87.016\" style=\"fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,777.536,422.196)\">")?;
    writeln!(writer, "          <path d=\"M0,0.008C0,17.531 -10.329,31.738 -23.07,31.738C-35.809,31.738 -46.139,17.531 -46.139,0.008C-46.139,-17.521 -35.809,-31.73 -23.07,-31.73C-10.329,-31.73 0,-17.521 0,0.008\" style=\"fill:white;fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,546.49,486.263)\">")?;
    writeln!(writer, "          <path d=\"M0,-93.046C0,-93.046 70.508,-124.265 89.753,-54.583C89.753,-54.583 109.912,26.635 31.851,31.219C31.851,31.219 -67.69,12.047 0,-93.046\" style=\"fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,581.903,423.351)\">")?;
    writeln!(writer, "          <path d=\"M0,0.002C0,18.074 -10.653,32.731 -23.794,32.731C-36.931,32.731 -47.586,18.074 -47.586,0.002C-47.586,-18.076 -36.931,-32.729 -23.794,-32.729C-10.653,-32.729 0,-18.076 0,0.002\" style=\"fill:white;fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,1002.23,778.679)\">")?;
    writeln!(writer, "          <path d=\"M0,-296.808C0,-296.808 -14.723,-238.165 -106.292,-176.541L-131.97,-170.523C-131.97,-170.523 -215.036,-322.004 -332.719,-151.302C-332.719,-151.302 -296.042,-172.656 -197.719,-146.652C-197.719,-146.652 -242.949,-77.426 -334.061,-79.553C-334.061,-79.553 -246.748,25.196 -113.881,-126.107C-113.881,-126.107 26.574,-180.422 37.964,-296.808L0,-296.808Z\" style=\"fill:rgb(247,76,0);fill-rule:nonzero;\"/>")?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "      </g>")?;
    writeln!(writer, "    </svg>")?;
    writeln!(writer, "    <div class=\"header-content\">")?;
    writeln!(writer, "      <h1>Crate Appraisal Report</h1>")?;
    writeln!(
        writer,
        "      <p class=\"subtitle\">Produced by cargo-aprz {} on {}</p>",
        env!("CARGO_PKG_VERSION"),
        date
    )?;
    writeln!(writer, "    </div>")?;
    writeln!(writer, "    <button class=\"theme-toggle\" onclick=\"toggleTheme()\" aria-label=\"Toggle theme\">")?;
    writeln!(writer, "      <svg id=\"theme-icon\" viewBox=\"0 0 24 24\"><path d=\"M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z\"/></svg>")?;
    writeln!(writer, "    </button>")?;
    writeln!(writer, "  </div>")?;
    Ok(())
}

#[expect(clippy::too_many_arguments, reason = "Summary stats are individual counts for clarity")]
fn write_summary<W: Write>(
    writer: &mut W,
    total: usize,
    low: usize,
    medium: usize,
    high: usize,
    not_evaluated: usize,
    high_risk_crates: &[(&str, String)],
    medium_risk_crates: &[(&str, String)],
) -> Result<()> {
    writeln!(writer, "  <div class=\"summary\">")?;
    writeln!(writer, "    <div class=\"summary-card total\"><div class=\"label\">Total Crates</div><div class=\"value\">{total}</div></div>")?;
    writeln!(writer, "    <div class=\"summary-card low\"><div class=\"label\">Low Risk</div><div class=\"value\">{low}</div></div>")?;
    writeln!(writer, "    <div class=\"summary-card medium\"><div class=\"label\">Medium Risk</div><div class=\"value\">{medium}</div></div>")?;
    writeln!(writer, "    <div class=\"summary-card high\"><div class=\"label\">High Risk</div><div class=\"value\">{high}</div></div>")?;
    if not_evaluated > 0 {
        writeln!(writer, "    <div class=\"summary-card not-eval\"><div class=\"label\">Not Evaluated</div><div class=\"value\">{not_evaluated}</div></div>")?;
    }
    writeln!(writer, "  </div>")?;

    if !high_risk_crates.is_empty() {
        write_risk_crate_list(writer, "high", "High Risk Crates", high_risk_crates)?;
    }
    if !medium_risk_crates.is_empty() {
        write_risk_crate_list(writer, "medium", "Medium Risk Crates", medium_risk_crates)?;
    }
    Ok(())
}

fn write_risk_crate_list<W: Write>(writer: &mut W, class: &str, title: &str, crate_entries: &[(&str, String)]) -> Result<()> {
    writeln!(writer, "  <div class=\"risk-list {class}\">")?;
    writeln!(writer, "    <strong>{title}</strong>")?;
    writeln!(writer, "    <div class=\"crate-names\">")?;
    for (name, version) in crate_entries {
        let anchor = crate_anchor_id(name, version);
        writeln!(
            writer,
            "      <a href=\"#{anchor}\"><span class=\"crate-name\">{} v{}</span></a>",
            html_escape(name),
            html_escape(version)
        )?;
    }
    writeln!(writer, "    </div>")?;
    writeln!(writer, "  </div>")?;
    Ok(())
}

fn write_filter_bar<W: Write>(writer: &mut W) -> Result<()> {
    writeln!(writer, "  <div class=\"filter-bar\">")?;
    writeln!(writer, "    <span class=\"label\">Filter by risk:</span>")?;
    writeln!(writer, "    <button class=\"filter-btn active\" data-filter=\"all\" onclick=\"filterByRisk('all')\">All</button>")?;
    writeln!(writer, "    <button class=\"filter-btn\" data-filter=\"low\" onclick=\"filterByRisk('low')\">Low</button>")?;
    writeln!(writer, "    <button class=\"filter-btn\" data-filter=\"medium\" onclick=\"filterByRisk('medium')\">Medium</button>")?;
    writeln!(writer, "    <button class=\"filter-btn\" data-filter=\"high\" onclick=\"filterByRisk('high')\">High</button>")?;
    writeln!(writer, "    <button class=\"filter-btn\" data-filter=\"none\" onclick=\"filterByRisk('none')\">Not Evaluated</button>")?;
    writeln!(writer, "  </div>")?;
    Ok(())
}

fn write_crate_card_header<W: Write>(writer: &mut W, crate_info: &ReportableCrate) -> Result<()> {
    let risk_class = crate_info.appraisal.as_ref().map_or("", |a| match a.risk {
        Risk::Low => " risk-low",
        Risk::Medium => " risk-medium",
        Risk::High => " risk-high",
    });
    writeln!(writer, "      <div class=\"crate-card-header{risk_class}\">")?;
    writeln!(
        writer,
        "        <span class=\"crate-title\">{} v{}</span>",
        html_escape(&crate_info.name),
        html_escape(&crate_info.version.to_string())
    )?;
    writeln!(writer, "        <span class=\"spacer\"></span>")?;
    if let Some(appraisal) = &crate_info.appraisal {
        let (class, label) = match appraisal.risk {
            Risk::Low => ("low", "LOW RISK"),
            Risk::Medium => ("medium", "MEDIUM RISK"),
            Risk::High => ("high", "HIGH RISK"),
        };
        writeln!(writer, "        <span class=\"header-right\">")?;
        writeln!(
            writer,
            "          <span class=\"appraisal-score\">score {:.0} · {}/{} points</span>",
            appraisal.score, appraisal.awarded_points, appraisal.available_points
        )?;
        writeln!(writer, "          <span class=\"risk-badge {class}\">{label}</span>")?;
        writeln!(writer, "        </span>")?;
    } else {
        writeln!(writer, "        <span class=\"risk-badge not-evaluated\">Not Evaluated</span>")?;
    }
    writeln!(writer, "      </div>")?;
    Ok(())
}

fn write_appraisal_table<W: Write>(writer: &mut W, appraisal: &crate::expr::Appraisal) -> Result<()> {
    writeln!(writer, "          <table>")?;
    writeln!(writer, "          <thead><tr><th>Expression</th><th>Result</th><th>Details</th></tr></thead>")?;
    writeln!(writer, "          <tbody>")?;
    for outcome in &appraisal.expression_outcomes {
        let (disp_class, disp_label) = match &outcome.disposition {
            ExpressionDisposition::True => ("passed", "PASSED"),
            ExpressionDisposition::False => ("failed", "FAILED"),
            ExpressionDisposition::Failed(_) => ("inconclusive", "INCONCLUSIVE"),
        };
        let detail = match &outcome.disposition {
            ExpressionDisposition::True | ExpressionDisposition::False => html_escape(&outcome.description),
            ExpressionDisposition::Failed(reason) => html_escape(reason),
        };
        writeln!(writer, "          <tr>")?;
        writeln!(writer, "            <td>{}</td>", html_escape(&outcome.name))?;
        writeln!(writer, "            <td><span class=\"disposition {disp_class}\">{disp_label}</span></td>")?;
        writeln!(writer, "            <td>{detail}</td>")?;
        writeln!(writer, "          </tr>")?;
    }
    writeln!(writer, "          </tbody>")?;
    writeln!(writer, "          </table>")?;
    Ok(())
}

fn write_metrics_category<W: Write>(
    writer: &mut W,
    category: MetricCategory,
    metrics_by_category: &crate::HashMap<MetricCategory, Vec<&'static str>>,
    metric_map: &crate::HashMap<&str, &crate::metrics::Metric>,
) -> Result<()> {
    let mut metric_buf = String::new();
    if let Some(category_metrics) = metrics_by_category.get(&category) {
        writeln!(writer, "          <table>")?;
        writeln!(writer, "            <tbody>")?;

        for &metric_name in category_metrics {
            let Some(m) = metric_map.get(metric_name) else { continue };

            writeln!(writer, "            <tr>")?;
            writeln!(
                writer,
                "              <td title=\"{}\">{}</td>",
                html_escape(m.description()),
                html_escape(metric_name)
            )?;
            write!(writer, "              <td>")?;
            if let Some(value) = &m.value {
                metric_buf.clear();
                common::write_metric_value(&mut metric_buf, value);

                if common::is_keywords_metric(metric_name) {
                    format_keywords_or_categories(&metric_buf, "keywords", writer)?;
                } else if common::is_categories_metric(metric_name) {
                    format_keywords_or_categories(&metric_buf, "categories", writer)?;
                } else if common::is_url(&metric_buf) {
                    write!(
                        writer,
                        "<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a>",
                        html_escape(&metric_buf),
                        html_escape(&metric_buf)
                    )?;
                } else {
                    write!(writer, "{}", html_escape(&metric_buf))?;
                }
            } else {
                write!(writer, "<span class=\"na\">n/a</span>")?;
            }
            writeln!(writer, "</td>")?;
            writeln!(writer, "            </tr>")?;
        }

        writeln!(writer, "            </tbody>")?;
        writeln!(writer, "          </table>")?;
    }
    Ok(())
}

fn crate_anchor_id(name: &str, version: &str) -> String {
    let mut id = String::with_capacity(name.len() + version.len() + 7);
    id.push_str("crate-");
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '-' {
            id.push(c);
        } else {
            id.push('-');
        }
    }
    id.push('-');
    for c in version.chars() {
        if c.is_ascii_alphanumeric() || c == '.' {
            id.push(c);
        } else {
            id.push('-');
        }
    }
    id
}

fn write_scripts<W: Write>(writer: &mut W, has_filter: bool) -> Result<()> {
    writeln!(writer, "  <script>")?;
    writeln!(writer, "    function getSystemTheme() {{")?;
    writeln!(writer, "      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    function updateIcon(theme) {{")?;
    writeln!(writer, "      const icon = document.getElementById('theme-icon');")?;
    writeln!(writer, "      if (theme === 'dark') {{")?;
    writeln!(writer, "        icon.innerHTML = '<circle cx=\"12\" cy=\"12\" r=\"4\" fill=\"currentColor\"/><path d=\"M12 1v2m0 18v2M4.22 4.22l1.42 1.42m12.72 12.72l1.42 1.42M1 12h2m18 0h2M4.22 19.78l1.42-1.42m12.72-12.72l1.42-1.42\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\"/>';")?;
    writeln!(writer, "      }} else {{")?;
    writeln!(writer, "        icon.innerHTML = '<path d=\"M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z\"/>';")?;
    writeln!(writer, "      }}")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    function applyTheme(theme) {{")?;
    writeln!(writer, "      document.body.classList.remove('dark-theme', 'light-theme');")?;
    writeln!(writer, "      document.body.classList.add(theme + '-theme');")?;
    writeln!(writer, "      updateIcon(theme);")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    function toggleTheme() {{")?;
    writeln!(writer, "      const currentTheme = localStorage.getItem('theme') || getSystemTheme();")?;
    writeln!(writer, "      const newTheme = currentTheme === 'dark' ? 'light' : 'dark';")?;
    writeln!(writer, "      localStorage.setItem('theme', newTheme);")?;
    writeln!(writer, "      applyTheme(newTheme);")?;
    writeln!(writer, "    }}")?;

    if has_filter {
        writeln!(writer, "    function filterByRisk(level) {{")?;
        writeln!(writer, "      document.querySelectorAll('.filter-btn').forEach(b => b.classList.remove('active'));")?;
        writeln!(writer, "      document.querySelector('.filter-btn[data-filter=\"' + level + '\"]').classList.add('active');")?;
        writeln!(writer, "      document.querySelectorAll('.crate-card').forEach(card => {{")?;
        writeln!(writer, "        if (level === 'all') {{ card.style.display = ''; }}")?;
        writeln!(writer, "        else {{ card.style.display = card.dataset.risk === level ? '' : 'none'; }}")?;
        writeln!(writer, "      }});")?;
        writeln!(writer, "    }}")?;
    }

    writeln!(writer, "    function switchTab(btn) {{")?;
    writeln!(writer, "      const tabs = btn.closest('.tabs');")?;
    writeln!(writer, "      tabs.querySelectorAll('.tab-btn').forEach(b => b.classList.remove('active'));")?;
    writeln!(writer, "      tabs.querySelectorAll('.tab-panel').forEach(p => p.classList.remove('active'));")?;
    writeln!(writer, "      btn.classList.add('active');")?;
    writeln!(writer, "      document.getElementById(btn.dataset.tab).classList.add('active');")?;
    writeln!(writer, "    }}")?;

    writeln!(writer, "    const savedTheme = localStorage.getItem('theme');")?;
    writeln!(writer, "    applyTheme(savedTheme || getSystemTheme());")?;
    writeln!(writer, "  </script>")?;
    Ok(())
}

fn html_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&#39;"),
            _ => result.push(c),
        }
    }
    result
}

fn format_keywords_or_categories<W: Write>(value: &str, url_type: &str, writer: &mut W) -> Result<()> {
    // Split by comma and filter empty items
    let items: Vec<&str> = value.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();

    if items.is_empty() {
        write!(writer, "{}", html_escape(value))?;
        return Ok(());
    }

    // Base URL doesn't need escaping (it's a constant string)
    let base_url = format!("https://crates.io/{url_type}/");

    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            write!(writer, ", ")?;
        }
        let url_encoded_item = utf8_percent_encode(item, NON_ALPHANUMERIC);
        let escaped_item = html_escape(item);
        write!(
            writer,
            "<a href=\"{base_url}{url_encoded_item}\" target=\"_blank\" rel=\"noopener noreferrer\">#{escaped_item}</a>"
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::Appraisal;
    use crate::metrics::{Metric, MetricDef, MetricValue};
    use chrono::TimeZone;
    use std::sync::Arc;

    fn test_timestamp() -> DateTime<Local> {
        Local.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap()
    }

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
    fn test_html_escape_basic() {
        assert_eq!(html_escape("hello"), "hello");
    }

    #[test]
    fn test_html_escape_ampersand() {
        assert_eq!(html_escape("A & B"), "A &amp; B");
    }

    #[test]
    fn test_html_escape_less_than() {
        assert_eq!(html_escape("<script>"), "&lt;script&gt;");
    }

    #[test]
    fn test_html_escape_quotes() {
        assert_eq!(html_escape("\"quoted\" and 'single'"), "&quot;quoted&quot; and &#39;single&#39;");
    }

    #[test]
    fn test_html_escape_all_special_chars() {
        assert_eq!(html_escape("<>&\"'"), "&lt;&gt;&amp;&quot;&#39;");
    }

    #[test]
    fn test_html_escape_empty() {
        assert_eq!(html_escape(""), "");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_empty_crates() {
        let crates: Vec<ReportableCrate> = vec![];
        let mut output = String::new();
        let result = generate(&crates, test_timestamp(), &mut output);
        result.unwrap();
        // Should still generate valid HTML structure
        assert!(output.contains("<!DOCTYPE html>"));
        assert!(output.contains("<html>"));
        assert!(output.contains("</html>"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_single_crate() {
        let crates = vec![create_test_crate("test_crate", "1.2.3", None)];
        let mut output = String::new();
        let result = generate(&crates, test_timestamp(), &mut output);
        result.unwrap();
        assert!(output.contains("<!DOCTYPE html>"));
        assert!(output.contains("Crate Appraisal Report"));
        assert!(output.contains("cargo-aprz"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_contains_ferris() {
        let crates = vec![create_test_crate("test", "1.0.0", None)];
        let mut output = String::new();
        let result = generate(&crates, test_timestamp(), &mut output);
        result.unwrap();
        // Should contain Ferris SVG
        assert!(output.contains("<svg class=\"ferris\""));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_contains_theme_toggle() {
        let crates = vec![create_test_crate("test", "1.0.0", None)];
        let mut output = String::new();
        let result = generate(&crates, test_timestamp(), &mut output);
        result.unwrap();
        // Should contain theme toggle functionality
        assert!(output.contains("toggleTheme"));
        assert!(output.contains("dark"));
        assert!(output.contains("light"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_contains_css_styles() {
        let crates = vec![create_test_crate("test", "1.0.0", None)];
        let mut output = String::new();
        let result = generate(&crates, test_timestamp(), &mut output);
        result.unwrap();
        // Should contain CSS styles
        assert!(output.contains("<style>"));
        assert!(output.contains("--bg-color"));
        assert!(output.contains("--text-color"));
    }

    #[test]
    fn test_format_keywords_or_categories_single() {
        let mut output = String::new();
        let result = format_keywords_or_categories("rust", "keywords", &mut output);
        result.unwrap();
        assert!(output.contains("#rust"));
        assert!(output.contains("https://crates.io/keywords/rust"));
    }

    #[test]
    fn test_format_keywords_or_categories_multiple() {
        let mut output = String::new();
        let result = format_keywords_or_categories("rust, web, async", "keywords", &mut output);
        result.unwrap();
        assert!(output.contains("#rust"));
        assert!(output.contains("#web"));
        assert!(output.contains("#async"));
        assert!(output.contains(", "));
    }

    #[test]
    fn test_format_keywords_or_categories_empty() {
        let mut output = String::new();
        let result = format_keywords_or_categories("", "keywords", &mut output);
        result.unwrap();
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_keywords_with_special_chars() {
        let mut output = String::new();
        let result = format_keywords_or_categories("A&B, C<D", "keywords", &mut output);
        result.unwrap();
        // Special characters should be escaped
        assert!(output.contains("&amp;"));
        assert!(output.contains("&lt;"));
    }

    #[test]
    fn test_format_categories() {
        let mut output = String::new();
        format_keywords_or_categories("web, cli", "categories", &mut output).unwrap();
        assert!(output.contains("https://crates.io/categories/"));
        assert!(output.contains("#web"));
        assert!(output.contains("#cli"));
    }

    // --- crate_anchor_id tests ---

    #[test]
    fn test_crate_anchor_id_simple() {
        assert_eq!(crate_anchor_id("tokio", "1.35.0"), "crate-tokio-1.35.0");
    }

    #[test]
    fn test_crate_anchor_id_with_hyphens() {
        assert_eq!(crate_anchor_id("my-crate", "0.1.0"), "crate-my-crate-0.1.0");
    }

    #[test]
    fn test_crate_anchor_id_special_chars_in_name() {
        // Underscores and other non-alphanumeric/non-hyphen chars become hyphens
        assert_eq!(crate_anchor_id("my_crate", "1.0.0"), "crate-my-crate-1.0.0");
    }

    #[test]
    fn test_crate_anchor_id_special_chars_in_version() {
        // Non-alphanumeric/non-dot chars in version become hyphens
        assert_eq!(crate_anchor_id("crate", "1.0.0-beta"), "crate-crate-1.0.0-beta");
    }

    // --- generate with all risk levels ---

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_with_all_risk_levels() {
        use crate::expr::ExpressionOutcome;

        let crates = vec![
            create_test_crate(
                "low_crate",
                "1.0.0",
                Some(Appraisal::new(
                    Risk::Low,
                    vec![ExpressionOutcome::new("check".into(), "All good".into(), ExpressionDisposition::True)],
                    1, 1, 100.0,
                )),
            ),
            create_test_crate(
                "medium_crate",
                "2.0.0",
                Some(Appraisal::new(
                    Risk::Medium,
                    vec![ExpressionOutcome::new("check".into(), "Partial".into(), ExpressionDisposition::False)],
                    2, 1, 50.0,
                )),
            ),
            create_test_crate(
                "high_crate",
                "3.0.0",
                Some(Appraisal::new(
                    Risk::High,
                    vec![ExpressionOutcome::new("check".into(), "Failed".into(), ExpressionDisposition::False)],
                    1, 0, 0.0,
                )),
            ),
            create_test_crate("unevaluated_crate", "0.1.0", None),
        ];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        // Summary section should be present
        assert!(output.contains("class=\"summary\""));
        assert!(output.contains("Low Risk"));
        assert!(output.contains("Medium Risk"));
        assert!(output.contains("High Risk"));
        assert!(output.contains("Not Evaluated"));

        // Filter bar should be present
        assert!(output.contains("data-filter="));
        assert!(output.contains("filterByRisk"));

        // Risk lists
        assert!(output.contains("high_crate"));
        assert!(output.contains("medium_crate"));

        // Card headers with risk classes
        assert!(output.contains("risk-low"));
        assert!(output.contains("risk-medium"));
        assert!(output.contains("risk-high"));

        // Risk badges
        assert!(output.contains("LOW RISK"));
        assert!(output.contains("MEDIUM RISK"));
        assert!(output.contains("HIGH RISK"));
        assert!(output.contains("Not Evaluated"));

        // Score text
        assert!(output.contains("score 100"));
        assert!(output.contains("score 50"));
        assert!(output.contains("score 0"));

        // Version prefix
        assert!(output.contains("v1.0.0"));
        assert!(output.contains("v2.0.0"));
        assert!(output.contains("v3.0.0"));
    }

    // --- appraisal table disposition variants ---

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_with_failed_disposition() {
        use crate::expr::ExpressionOutcome;

        let crates = vec![create_test_crate(
            "err_crate",
            "1.0.0",
            Some(Appraisal::new(
                Risk::Low,
                vec![ExpressionOutcome::new(
                    "broken_check".into(),
                    "desc".into(),
                    ExpressionDisposition::Failed("variable not found".into()),
                )],
                0, 0, 100.0,
            )),
        )];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        assert!(output.contains("INCONCLUSIVE"));
        assert!(output.contains("variable not found"));
        assert!(output.contains("broken_check"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_appraisal_passed_and_failed() {
        use crate::expr::ExpressionOutcome;

        let crates = vec![create_test_crate(
            "mixed_crate",
            "1.0.0",
            Some(Appraisal::new(
                Risk::Medium,
                vec![
                    ExpressionOutcome::new("ok_check".into(), "Passed check".into(), ExpressionDisposition::True),
                    ExpressionOutcome::new("bad_check".into(), "Failed check".into(), ExpressionDisposition::False),
                ],
                2, 1, 50.0,
            )),
        )];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        assert!(output.contains("PASSED"));
        assert!(output.contains("FAILED"));
        assert!(output.contains("ok_check"));
        assert!(output.contains("bad_check"));
    }

    // --- crate with empty expression outcomes (appraisal but no appraisal tab) ---

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_appraisal_no_outcomes() {
        let crates = vec![create_test_crate(
            "empty_eval",
            "1.0.0",
            Some(Appraisal::new(Risk::Low, vec![], 0, 0, 100.0)),
        )];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        // Should still have the card header with risk badge
        assert!(output.contains("LOW RISK"));
        // Should not have the appraisal tab button, but should have metric tabs
        assert!(!output.contains("data-tab=\"card-0-appraisal\""));
        // The first metric tab should be active
        assert!(output.contains("tab-btn active"));
    }

    // --- metrics rendering: URL, n/a, categories ---

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_with_url_metric() {
        static URL_DEF: MetricDef = MetricDef {
            name: "crate.repository",
            description: "Repository URL",
            category: MetricCategory::Metadata,
            extractor: |_| None,
            default_value: || None,
        };

        let metrics = vec![
            Metric::with_value(&NAME_DEF, MetricValue::String("url_crate".into())),
            Metric::with_value(&VERSION_DEF, MetricValue::String("1.0.0".into())),
            Metric::with_value(&URL_DEF, MetricValue::String("https://github.com/example/repo".into())),
        ];
        let crates = vec![ReportableCrate::new(
            "url_crate".into(),
            Arc::new("1.0.0".parse().unwrap()),
            metrics,
            None,
        )];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        assert!(output.contains("href=\"https://github.com/example/repo\""));
        assert!(output.contains("target=\"_blank\""));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_with_na_metric() {
        static OPT_DEF: MetricDef = MetricDef {
            name: "crate.optional",
            description: "Optional metric",
            category: MetricCategory::Metadata,
            extractor: |_| None,
            default_value: || None,
        };

        let metrics = vec![
            Metric::with_value(&NAME_DEF, MetricValue::String("na_crate".into())),
            Metric::with_value(&VERSION_DEF, MetricValue::String("1.0.0".into())),
            Metric::new(&OPT_DEF),
        ];
        let crates = vec![ReportableCrate::new(
            "na_crate".into(),
            Arc::new("1.0.0".parse().unwrap()),
            metrics,
            None,
        )];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        assert!(output.contains("<span class=\"na\">n/a</span>"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_with_categories_metric() {
        static CAT_DEF: MetricDef = MetricDef {
            name: "crate.categories",
            description: "Crate categories",
            category: MetricCategory::Metadata,
            extractor: |_| None,
            default_value: || None,
        };

        let metrics = vec![
            Metric::with_value(&NAME_DEF, MetricValue::String("cat_crate".into())),
            Metric::with_value(&VERSION_DEF, MetricValue::String("1.0.0".into())),
            Metric::with_value(
                &CAT_DEF,
                MetricValue::List(vec![MetricValue::String("web".into()), MetricValue::String("async".into())]),
            ),
        ];
        let crates = vec![ReportableCrate::new(
            "cat_crate".into(),
            Arc::new("1.0.0".parse().unwrap()),
            metrics,
            None,
        )];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        assert!(output.contains("https://crates.io/categories/"));
        assert!(output.contains("#web"));
    }

    // --- generate without appraisals (no filter bar, no summary) ---

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_no_appraisals_no_filter_bar() {
        let crates = vec![
            create_test_crate("crate_a", "1.0.0", None),
            create_test_crate("crate_b", "2.0.0", None),
        ];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        // No summary or filter bar when no appraisals
        assert!(!output.contains("class=\"summary\""));
        assert!(!output.contains("data-filter="));
        // filterByRisk function should not be present
        assert!(!output.contains("filterByRisk"));
    }

    // --- anchor navigation ---

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_anchor_links_in_summary() {
        use crate::expr::ExpressionOutcome;

        let crates = vec![create_test_crate(
            "risky_crate",
            "0.5.0",
            Some(Appraisal::new(
                Risk::High,
                vec![ExpressionOutcome::new("check".into(), "desc".into(), ExpressionDisposition::False)],
                1, 0, 0.0,
            )),
        )];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        // Summary should link to crate card anchor
        assert!(output.contains("href=\"#crate-risky-crate-0.5.0\""));
        // Card should have matching id
        assert!(output.contains("id=\"crate-risky-crate-0.5.0\""));
    }

    // --- special characters in crate name ---

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_html_escapes_crate_name() {
        let crates = vec![create_test_crate("crate<xss>", "1.0.0", None)];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        // Name should be escaped
        assert!(output.contains("crate&lt;xss&gt;"));
        assert!(!output.contains("crate<xss>"));
    }

    // --- switchTab script ---

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTimeZoneInformationForYear")]
    fn test_generate_contains_tab_switching_script() {
        use crate::expr::ExpressionOutcome;

        let crates = vec![create_test_crate(
            "tabbed",
            "1.0.0",
            Some(Appraisal::new(
                Risk::Low,
                vec![ExpressionOutcome::new("c".into(), "d".into(), ExpressionDisposition::True)],
                1, 1, 100.0,
            )),
        )];
        let mut output = String::new();
        generate(&crates, test_timestamp(), &mut output).unwrap();

        assert!(output.contains("switchTab"));
        assert!(output.contains("tab-btn"));
        assert!(output.contains("tab-panel"));
    }

    // --- write_risk_crate_list ---

    #[test]
    fn test_write_risk_crate_list_renders_entries() {
        let entries = vec![("tokio", "1.35.0".to_string()), ("serde", "1.0.195".to_string())];
        let mut output = String::new();
        write_risk_crate_list(&mut output, "high", "High Risk Crates", &entries).unwrap();

        assert!(output.contains("High Risk Crates"));
        assert!(output.contains("tokio"));
        assert!(output.contains("v1.35.0"));
        assert!(output.contains("serde"));
        assert!(output.contains("v1.0.195"));
        assert!(output.contains("href=\"#crate-tokio-1.35.0\""));
        assert!(output.contains("href=\"#crate-serde-1.0.195\""));
    }

    #[test]
    fn test_write_risk_crate_list_empty() {
        let entries: Vec<(&str, String)> = vec![];
        let mut output = String::new();
        write_risk_crate_list(&mut output, "medium", "Medium Risk Crates", &entries).unwrap();

        assert!(output.contains("Medium Risk Crates"));
        assert!(output.contains("crate-names"));
    }
}
