use super::{ReportableCrate, common};
use crate::Result;
use crate::expr::Risk;
use crate::metrics::MetricCategory;
use chrono::{DateTime, Local};
use core::fmt::Write;
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use strum::IntoEnumIterator;

#[expect(clippy::too_many_lines, reason = "HTML template generation naturally requires many lines")]
pub fn generate<W: Write>(crates: &[ReportableCrate], timestamp: DateTime<Local>, writer: &mut W) -> Result<()> {
    writeln!(writer, "<!DOCTYPE html>")?;
    writeln!(writer, "<html>")?;
    writeln!(writer, "<head>")?;
    writeln!(writer, "  <meta charset=\"UTF-8\">")?;
    writeln!(writer, "  <title>Crate Metrics</title>")?;
    writeln!(writer, "  <link rel=\"icon\" type=\"image/svg+xml\" href=\"data:image/svg+xml,%3Csvg viewBox='0 0 1200 800' xmlns='http://www.w3.org/2000/svg'%3E%3Cg%3E%3Cg transform='matrix(1,0,0,1,654.172,668.359)'%3E%3Cpath d='M0,-322.648C-114.597,-322.648 -218.172,-308.869 -296.172,-286.419L-296.172,-291.49C-374.172,-266.395 -423.853,-231.531 -423.853,-192.984C-423.853,-186.907 -422.508,-180.922 -420.15,-175.053L-428.134,-160.732C-428.134,-160.732 -434.547,-152.373 -423.199,-134.733C-413.189,-119.179 -363.035,-58.295 -336.571,-26.413C-325.204,-10.065 -317.488,0 -316.814,-0.973C-315.753,-2.516 -323.878,-33.202 -346.453,-68.215C-356.986,-87.02 -369.811,-111.934 -377.361,-130.335C-356.28,-116.993 -328.172,-104.89 -296.172,-94.474L-296.172,-94.633C-218.172,-72.18 -114.597,-58.404 0,-58.404C131.156,-58.404 248.828,-76.45 327.828,-104.895L327.828,-276.153C248.828,-304.6 131.156,-322.648 0,-322.648' fill='%23a52b00'/%3E%3C/g%3E%3Cg transform='matrix(1,0,0,1,1177.87,277.21)'%3E%3Cpath d='M0,227.175L-88.296,162.132C-89.126,159.237 -89.956,156.345 -90.812,153.474L-61.81,111.458C-58.849,107.184 -58.252,101.629 -60.175,96.755C-62.1,91.905 -66.311,88.428 -71.292,87.576L-120.335,79.255C-122.233,75.376 -124.225,71.557 -126.224,67.771L-105.62,20.599C-103.501,15.793 -103.947,10.209 -106.759,5.848C-109.556,1.465 -114.31,-1.094 -119.376,-0.895L-169.146,0.914C-171.723,-2.442 -174.34,-5.766 -177.012,-9.032L-165.574,-59.592C-164.415,-64.724 -165.876,-70.1 -169.453,-73.83C-173.008,-77.546 -178.175,-79.084 -183.089,-77.88L-231.567,-65.961C-234.707,-68.736 -237.897,-71.474 -241.126,-74.157L-239.381,-126.064C-239.193,-131.318 -241.643,-136.311 -245.849,-139.227C-250.053,-142.161 -255.389,-142.603 -259.987,-140.423L-305.213,-118.921C-308.853,-121.011 -312.515,-123.081 -316.218,-125.084L-324.209,-176.232C-325.021,-181.413 -328.355,-185.816 -333.024,-187.826C-337.679,-189.848 -343.014,-189.193 -347.101,-186.116L-387.422,-155.863C-391.392,-157.181 -395.38,-158.446 -399.418,-159.655L-416.798,-208.159C-418.564,-213.104 -422.64,-216.735 -427.608,-217.756C-432.561,-218.768 -437.656,-217.053 -441.091,-213.217L-475.029,-175.246C-479.133,-175.717 -483.239,-176.147 -487.356,-176.505L-513.564,-220.659C-516.22,-225.131 -520.908,-227.852 -525.961,-227.852C-531.002,-227.852 -535.7,-225.131 -538.333,-220.659L-564.547,-176.505C-568.666,-176.147 -572.791,-175.717 -576.888,-175.246L-610.831,-213.217C-614.268,-217.053 -619.382,-218.768 -624.318,-217.756C-629.284,-216.721 -633.363,-213.104 -635.124,-208.159L-652.517,-159.655C-656.544,-158.446 -660.534,-157.173 -664.514,-155.863L-704.822,-186.116C-708.92,-189.204 -714.254,-189.857 -718.92,-187.826C-723.57,-185.816 -726.917,-181.413 -727.723,-176.232L-735.72,-125.084C-739.42,-123.081 -743.083,-121.022 -746.734,-118.921L-791.956,-140.423C-796.548,-142.612 -801.908,-142.161 -806.091,-139.227C-810.292,-136.311 -812.747,-131.318 -812.557,-126.064L-810.821,-74.157C-814.04,-71.474 -817.224,-68.736 -820.379,-65.961L-868.849,-77.88C-873.774,-79.075 -878.935,-77.546 -882.499,-73.83C-886.084,-70.1 -887.538,-64.724 -886.384,-59.592L-874.969,-9.032C-877.618,-5.753 -880.239,-2.442 -882.808,0.914L-932.579,-0.895C-937.602,-1.043 -942.396,1.465 -945.202,5.848C-948.014,10.209 -948.439,15.793 -946.348,20.599L-925.729,67.771C-927.732,71.557 -929.721,75.376 -931.635,79.255L-980.675,87.576C-985.657,88.417 -989.858,91.892 -991.795,96.755C-993.72,101.629 -993.095,107.184 -990.156,111.458L-961.146,153.474C-961.37,154.215 -961.576,154.964 -961.799,155.707L-1043.82,242.829C-1043.82,242.829 -1056.38,252.68 -1038.09,275.831C-1021.95,296.252 -939.097,377.207 -895.338,419.62C-876.855,441.152 -864.195,454.486 -862.872,453.332C-860.784,451.5 -871.743,412.326 -908.147,366.362C-936.207,325.123 -972.625,261.696 -964.086,254.385C-964.086,254.385 -954.372,242.054 -934.882,233.178C-934.169,233.749 -935.619,232.613 -934.882,233.178C-934.882,233.178 -523.568,422.914 -142.036,236.388C-98.452,228.571 -72.068,251.917 -72.068,251.917C-62.969,257.193 -86.531,322.412 -105.906,365.583C-132.259,414.606 -136.123,452.859 -133.888,454.185C-132.479,455.027 -122.89,440.438 -109.214,417.219C-75.469,370.196 -11.675,280.554 0,258.781C13.239,234.094 0,227.175 0,227.175' fill='%23f74c00'/%3E%3C/g%3E%3C/g%3E%3C/svg%3E\">")?;
    writeln!(writer, "  <style>")?;
    writeln!(writer, "    :root {{")?;
    writeln!(writer, "      --bg-color: #e5e7eb;")?;
    writeln!(writer, "      --card-bg: #ffffff;")?;
    writeln!(writer, "      --text-color: #1a202c;")?;
    writeln!(writer, "      --border-color: #e2e8f0;")?;
    writeln!(writer, "      --category-bg: #fed7aa;")?;
    writeln!(writer, "      --category-border: #cbd5e0;")?;
    writeln!(writer, "      --hover-bg: rgba(66, 153, 225, 0.04);")?;
    writeln!(writer, "      --metric-cell-bg: #fafafa;")?;
    writeln!(writer, "      --accent-color: #4299e1;")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    @media (prefers-color-scheme: dark) {{")?;
    writeln!(writer, "      :root {{")?;
    writeln!(writer, "        --bg-color: #374151;")?;
    writeln!(writer, "        --card-bg: #000000;")?;
    writeln!(writer, "        --text-color: #e5e7eb;")?;
    writeln!(writer, "        --border-color: #4b5563;")?;
    writeln!(writer, "        --category-bg: #a52b00;")?;
    writeln!(writer, "        --category-border: #6b7280;")?;
    writeln!(writer, "        --hover-bg: rgba(99, 179, 237, 0.08);")?;
    writeln!(writer, "        --metric-cell-bg: #0a0a0a;")?;
    writeln!(writer, "        --accent-color: #60a5fa;")?;
    writeln!(writer, "      }}")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    body.dark-theme {{")?;
    writeln!(writer, "      --bg-color: #374151;")?;
    writeln!(writer, "      --card-bg: #000000;")?;
    writeln!(writer, "      --text-color: #e5e7eb;")?;
    writeln!(writer, "      --border-color: #4b5563;")?;
    writeln!(writer, "      --category-bg: #a52b00;")?;
    writeln!(writer, "      --category-border: #6b7280;")?;
    writeln!(writer, "      --hover-bg: rgba(99, 179, 237, 0.08);")?;
    writeln!(writer, "      --metric-cell-bg: #0a0a0a;")?;
    writeln!(writer, "      --accent-color: #60a5fa;")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    body.light-theme {{")?;
    writeln!(writer, "      --bg-color: #e5e7eb;")?;
    writeln!(writer, "      --card-bg: #ffffff;")?;
    writeln!(writer, "      --text-color: #1a202c;")?;
    writeln!(writer, "      --border-color: #e2e8f0;")?;
    writeln!(writer, "      --category-bg: #fed7aa;")?;
    writeln!(writer, "      --category-border: #cbd5e0;")?;
    writeln!(writer, "      --hover-bg: rgba(66, 153, 225, 0.04);")?;
    writeln!(writer, "      --metric-cell-bg: #fafafa;")?;
    writeln!(writer, "      --accent-color: #4299e1;")?;
    writeln!(writer, "    }}")?;
    writeln!(
        writer,
        "    body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Helvetica, Arial, sans-serif; margin: 0; padding: 32px; background-color: var(--bg-color); color: var(--text-color); transition: background-color 0.3s ease, color 0.3s ease; }}"
    )?;
    writeln!(writer, "    body.dark-theme {{ color-scheme: dark; }}")?;
    writeln!(writer, "    body.light-theme {{ color-scheme: light; }}")?;
    writeln!(
        writer,
        "    .header {{ display: grid; grid-template-columns: 1fr auto 1fr; align-items: flex-start; gap: 20px; margin-bottom: 32px; }}"
    )?;
    writeln!(writer, "    .header-content {{}}")?;
    writeln!(
        writer,
        "    h1 {{ color: var(--text-color); margin-bottom: 4px; font-size: 28px; font-weight: 600; letter-spacing: -0.5px; margin-top: 0; }}"
    )?;
    writeln!(
        writer,
        "    .subtitle {{ color: var(--text-color); font-size: 13px; opacity: 0.6; margin-top: 0; margin-bottom: 0; font-weight: 400; }}"
    )?;
    writeln!(
        writer,
        "    .ferris {{ width: 60px; height: 40px; margin: 0 20px; flex-shrink: 0; }}"
    )?;
    writeln!(
        writer,
        "    .theme-toggle {{ background: none; border: 2px solid var(--border-color); border-radius: 8px; width: 44px; height: 44px; cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all 0.2s ease; flex-shrink: 0; justify-self: end; }}"
    )?;
    writeln!(
        writer,
        "    .theme-toggle:hover {{ border-color: var(--accent-color); background-color: var(--hover-bg); }}"
    )?;
    writeln!(
        writer,
        "    .theme-toggle svg {{ width: 20px; height: 20px; fill: var(--text-color); opacity: 0.8; }}"
    )?;
    writeln!(
        writer,
        "    .table-wrapper {{ background-color: var(--card-bg); border-radius: 12px; box-shadow: 0 1px 3px rgba(0,0,0,0.06), 0 4px 12px rgba(0,0,0,0.04); overflow: hidden; }}"
    )?;
    writeln!(
        writer,
        "    table {{ border-collapse: collapse; width: 100%; background-color: var(--card-bg); }}"
    )?;
    writeln!(
        writer,
        "    td {{ border-bottom: 1px solid var(--border-color); padding: 14px 16px; text-align: left; font-size: 14px; }}"
    )?;
    writeln!(
        writer,
        "    .category-header {{ background-color: var(--category-bg); font-weight: bold; font-size: 11px; letter-spacing: 0.8px; text-transform: uppercase; color: var(--text-color); opacity: 0.8; height: 44px; }}"
    )?;
    writeln!(
        writer,
        "    .category-header td {{ padding: 12px 16px; border-top: 3px solid var(--category-border); border-bottom: 1px solid var(--category-border); }}"
    )?;
    writeln!(writer, "    .category-spacer {{ height: 16px; }}")?;
    writeln!(
        writer,
        "    .category-spacer td {{ background-color: var(--bg-color); border: none; padding: 0; }}"
    )?;
    writeln!(
        writer,
        "    .category-header td:first-child {{ position: sticky; left: 0; z-index: 15; background-color: var(--category-bg); border-right: 1px solid var(--border-color); }}"
    )?;
    writeln!(
        writer,
        "    .category-header td:not(:first-child) {{ background-color: var(--category-bg); }}"
    )?;
    writeln!(writer, "    .metric-row:hover {{ background-color: var(--hover-bg); }}")?;
    writeln!(
        writer,
        "    .metric-row td:first-child {{ font-weight: 500; background-color: var(--metric-cell-bg); position: sticky; left: 0; z-index: 5; border-right: 1px solid var(--border-color); }}"
    )?;
    writeln!(
        writer,
        "    .metric-row:hover td:first-child {{ background-color: var(--hover-bg); }}"
    )?;
    writeln!(writer, "    .metric-row:last-child td {{ border-bottom: none; }}")?;
    writeln!(
        writer,
        "    .low-risk {{ background-color: #c8e6c9; color: #2e7d32; font-weight: 600; padding: 3px 8px; border-radius: 4px; font-size: 11px; }}"
    )?;
    writeln!(
        writer,
        "    .medium-risk {{ background-color: #fff9c4; color: #f57f17; font-weight: 600; padding: 3px 8px; border-radius: 4px; font-size: 11px; }}"
    )?;
    writeln!(
        writer,
        "    .high-risk {{ background-color: #ffcdd2; color: #c62828; font-weight: 600; padding: 3px 8px; border-radius: 4px; font-size: 11px; }}"
    )?;
    writeln!(
        writer,
        "    .not-evaluated {{ background-color: #fff9c4; color: #f57f17; padding: 3px 8px; border-radius: 4px; font-size: 11px; }}"
    )?;
    writeln!(writer, "    .na {{ color: #a0aec0; font-style: italic; font-size: 13px; }}")?;
    writeln!(
        writer,
        "    .table-container {{ overflow-x: auto; max-height: calc(100vh - 160px); }}"
    )?;
    writeln!(writer, "    a {{ color: var(--accent-color); text-decoration: none; }}")?;
    writeln!(writer, "    a:hover {{ text-decoration: underline; }}")?;
    writeln!(writer, "    @media (prefers-color-scheme: dark) {{")?;
    writeln!(writer, "      .low-risk {{ background-color: #1b5e20; color: #a5d6a7; }}")?;
    writeln!(writer, "      .medium-risk {{ background-color: #f57f17; color: #212121; }}")?;
    writeln!(writer, "      .high-risk {{ background-color: #b71c1c; color: #ef9a9a; }}")?;
    writeln!(writer, "      .not-evaluated {{ background-color: #f57f17; color: #212121; }}")?;
    writeln!(writer, "      .na {{ color: #8b949e; }}")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "  </style>")?;
    writeln!(writer, "</head>")?;
    writeln!(writer, "<body>")?;
    writeln!(writer, "  <div class=\"header\">")?;
    writeln!(writer, "    <div class=\"header-content\">")?;
    writeln!(writer, "      <h1>Crate Metrics</h1>")?;
    let date = timestamp.format("%Y-%m-%d").to_string();
    writeln!(
        writer,
        "      <p class=\"subtitle\">Produced by cargo-aprz {} on {}</p>",
        env!("CARGO_PKG_VERSION"),
        date
    )?;
    writeln!(writer, "    </div>")?;
    writeln!(
        writer,
        "    <svg class=\"ferris\" viewBox=\"0 0 1200 800\" xmlns=\"http://www.w3.org/2000/svg\" style=\"fill-rule:evenodd;clip-rule:evenodd;stroke-linejoin:round;stroke-miterlimit:1.41421;\">"
    )?;
    writeln!(writer, "      <g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,654.172,668.359)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,-322.648C-114.597,-322.648 -218.172,-308.869 -296.172,-286.419L-296.172,-291.49C-374.172,-266.395 -423.853,-231.531 -423.853,-192.984C-423.853,-186.907 -422.508,-180.922 -420.15,-175.053L-428.134,-160.732C-428.134,-160.732 -434.547,-152.373 -423.199,-134.733C-413.189,-119.179 -363.035,-58.295 -336.571,-26.413C-325.204,-10.065 -317.488,0 -316.814,-0.973C-315.753,-2.516 -323.878,-33.202 -346.453,-68.215C-356.986,-87.02 -369.811,-111.934 -377.361,-130.335C-356.28,-116.993 -328.172,-104.89 -296.172,-94.474L-296.172,-94.633C-218.172,-72.18 -114.597,-58.404 0,-58.404C131.156,-58.404 248.828,-76.45 327.828,-104.895L327.828,-276.153C248.828,-304.6 131.156,-322.648 0,-322.648\" style=\"fill:rgb(165,43,0);fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,1099.87,554.94)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,-50.399L-13.433,-78.227C-13.362,-79.283 -13.309,-80.341 -13.309,-81.402C-13.309,-112.95 -46.114,-142.022 -101.306,-165.303L-101.306,2.499C-75.555,-8.365 -54.661,-20.485 -39.72,-33.538C-44.118,-15.855 -59.157,19.917 -71.148,45.073C-90.855,81.054 -97.993,112.376 -97.077,113.926C-96.493,114.904 -89.77,104.533 -79.855,87.726C-56.783,54.85 -13.063,-7.914 -4.325,-23.901C5.574,-42.024 0,-50.399 0,-50.399\" style=\"fill:rgb(165,43,0);fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,1177.87,277.21)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,227.175L-88.296,162.132C-89.126,159.237 -89.956,156.345 -90.812,153.474L-61.81,111.458C-58.849,107.184 -58.252,101.629 -60.175,96.755C-62.1,91.905 -66.311,88.428 -71.292,87.576L-120.335,79.255C-122.233,75.376 -124.225,71.557 -126.224,67.771L-105.62,20.599C-103.501,15.793 -103.947,10.209 -106.759,5.848C-109.556,1.465 -114.31,-1.094 -119.376,-0.895L-169.146,0.914C-171.723,-2.442 -174.34,-5.766 -177.012,-9.032L-165.574,-59.592C-164.415,-64.724 -165.876,-70.1 -169.453,-73.83C-173.008,-77.546 -178.175,-79.084 -183.089,-77.88L-231.567,-65.961C-234.707,-68.736 -237.897,-71.474 -241.126,-74.157L-239.381,-126.064C-239.193,-131.318 -241.643,-136.311 -245.849,-139.227C-250.053,-142.161 -255.389,-142.603 -259.987,-140.423L-305.213,-118.921C-308.853,-121.011 -312.515,-123.081 -316.218,-125.084L-324.209,-176.232C-325.021,-181.413 -328.355,-185.816 -333.024,-187.826C-337.679,-189.848 -343.014,-189.193 -347.101,-186.116L-387.422,-155.863C-391.392,-157.181 -395.38,-158.446 -399.418,-159.655L-416.798,-208.159C-418.564,-213.104 -422.64,-216.735 -427.608,-217.756C-432.561,-218.768 -437.656,-217.053 -441.091,-213.217L-475.029,-175.246C-479.133,-175.717 -483.239,-176.147 -487.356,-176.505L-513.564,-220.659C-516.22,-225.131 -520.908,-227.852 -525.961,-227.852C-531.002,-227.852 -535.7,-225.131 -538.333,-220.659L-564.547,-176.505C-568.666,-176.147 -572.791,-175.717 -576.888,-175.246L-610.831,-213.217C-614.268,-217.053 -619.382,-218.768 -624.318,-217.756C-629.284,-216.721 -633.363,-213.104 -635.124,-208.159L-652.517,-159.655C-656.544,-158.446 -660.534,-157.173 -664.514,-155.863L-704.822,-186.116C-708.92,-189.204 -714.254,-189.857 -718.92,-187.826C-723.57,-185.816 -726.917,-181.413 -727.723,-176.232L-735.72,-125.084C-739.42,-123.081 -743.083,-121.022 -746.734,-118.921L-791.956,-140.423C-796.548,-142.612 -801.908,-142.161 -806.091,-139.227C-810.292,-136.311 -812.747,-131.318 -812.557,-126.064L-810.821,-74.157C-814.04,-71.474 -817.224,-68.736 -820.379,-65.961L-868.849,-77.88C-873.774,-79.075 -878.935,-77.546 -882.499,-73.83C-886.084,-70.1 -887.538,-64.724 -886.384,-59.592L-874.969,-9.032C-877.618,-5.753 -880.239,-2.442 -882.808,0.914L-932.579,-0.895C-937.602,-1.043 -942.396,1.465 -945.202,5.848C-948.014,10.209 -948.439,15.793 -946.348,20.599L-925.729,67.771C-927.732,71.557 -929.721,75.376 -931.635,79.255L-980.675,87.576C-985.657,88.417 -989.858,91.892 -991.795,96.755C-993.72,101.629 -993.095,107.184 -990.156,111.458L-961.146,153.474C-961.37,154.215 -961.576,154.964 -961.799,155.707L-1043.82,242.829C-1043.82,242.829 -1056.38,252.68 -1038.09,275.831C-1021.95,296.252 -939.097,377.207 -895.338,419.62C-876.855,441.152 -864.195,454.486 -862.872,453.332C-860.784,451.5 -871.743,412.326 -908.147,366.362C-936.207,325.123 -972.625,261.696 -964.086,254.385C-964.086,254.385 -954.372,242.054 -934.882,233.178C-934.169,233.749 -935.619,232.613 -934.882,233.178C-934.882,233.178 -523.568,422.914 -142.036,236.388C-98.452,228.571 -72.068,251.917 -72.068,251.917C-62.969,257.193 -86.531,322.412 -105.906,365.583C-132.259,414.606 -136.123,452.859 -133.888,454.185C-132.479,455.027 -122.89,440.438 -109.214,417.219C-75.469,370.196 -11.675,280.554 0,258.781C13.239,234.094 0,227.175 0,227.175\" style=\"fill:rgb(247,76,0);fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,795.856,464.937)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,159.631C1.575,158.289 2.4,157.492 2.4,157.492L-132.25,144.985C-22.348,0 65.618,116.967 74.988,129.879L74.988,159.631L0,159.631Z\" style=\"fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,278.418,211.791)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,253.04C0,253.04 -111.096,209.79 -129.876,163.242C-129.876,163.242 0.515,59.525 -155.497,-50.644L-159.726,89.773C-159.726,89.773 -205.952,45.179 -203.912,-32.91C-203.912,-32.91 -347.685,36.268 -179.436,158.667C-179.436,158.667 -173.76,224.365 -22.459,303.684L0,253.04Z\" style=\"fill:rgb(247,76,0);fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,729.948,492.523)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,-87.016C0,-87.016 41.104,-132.025 82.21,-87.016C82.21,-87.016 114.507,-27.003 82.21,3C82.21,3 29.36,45.009 0,3C0,3 -35.232,-30.006 0,-87.016\" style=\"fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,777.536,422.196)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,0.008C0,17.531 -10.329,31.738 -23.07,31.738C-35.809,31.738 -46.139,17.531 -46.139,0.008C-46.139,-17.521 -35.809,-31.73 -23.07,-31.73C-10.329,-31.73 0,-17.521 0,0.008\" style=\"fill:white;fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,546.49,486.263)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,-93.046C0,-93.046 70.508,-124.265 89.753,-54.583C89.753,-54.583 109.912,26.635 31.851,31.219C31.851,31.219 -67.69,12.047 0,-93.046\" style=\"fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,581.903,423.351)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,0.002C0,18.074 -10.653,32.731 -23.794,32.731C-36.931,32.731 -47.586,18.074 -47.586,0.002C-47.586,-18.076 -36.931,-32.729 -23.794,-32.729C-10.653,-32.729 0,-18.076 0,0.002\" style=\"fill:white;fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "        <g transform=\"matrix(1,0,0,1,1002.23,778.679)\">")?;
    writeln!(
        writer,
        "          <path d=\"M0,-296.808C0,-296.808 -14.723,-238.165 -106.292,-176.541L-131.97,-170.523C-131.97,-170.523 -215.036,-322.004 -332.719,-151.302C-332.719,-151.302 -296.042,-172.656 -197.719,-146.652C-197.719,-146.652 -242.949,-77.426 -334.061,-79.553C-334.061,-79.553 -246.748,25.196 -113.881,-126.107C-113.881,-126.107 26.574,-180.422 37.964,-296.808L0,-296.808Z\" style=\"fill:rgb(247,76,0);fill-rule:nonzero;\"/>"
    )?;
    writeln!(writer, "        </g>")?;
    writeln!(writer, "      </g>")?;
    writeln!(writer, "    </svg>")?;
    writeln!(
        writer,
        "    <button class=\"theme-toggle\" onclick=\"toggleTheme()\" aria-label=\"Toggle theme\">"
    )?;
    writeln!(
        writer,
        "      <svg id=\"theme-icon\" viewBox=\"0 0 24 24\"><path d=\"M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z\"/></svg>"
    )?;
    writeln!(writer, "    </button>")?;
    writeln!(writer, "  </div>")?;

    // Group metrics by category across all crates
    let metrics_by_category = common::group_all_metrics_by_category(crates.iter().map(|c| c.metrics.as_slice()));

    writeln!(writer, "  <div class=\"table-wrapper\">")?;
    writeln!(writer, "    <div class=\"table-container\">")?;
    writeln!(writer, "      <table>")?;
    writeln!(writer, "        <tbody>")?;

    // Add appraisal results if any crate has some
    let has_appraisals = crates.iter().any(|c| c.appraisal.is_some());
    if has_appraisals {
        writeln!(writer, "          <tr class=\"metric-row\">")?;
        writeln!(writer, "            <td><strong>Appraisals</strong></td>")?;
        for crate_info in crates {
            write!(writer, "            <td>")?;
            if let Some(eval) = &crate_info.appraisal {
                let status_str = common::format_appraisal_status(eval);
                let class = match eval.risk {
                    Risk::Low => "low-risk",
                    Risk::Medium => "medium-risk",
                    Risk::High => "high-risk",
                };
                write!(writer, "<span class=\"{class}\">{status_str}</span>")?;
            } else {
                write!(writer, "<span class=\"na\">n/a</span>")?;
            }
            writeln!(writer, "</td>")?;
        }
        writeln!(writer, "          </tr>")?;

        // Reasons row
        writeln!(writer, "          <tr class=\"metric-row\">")?;
        writeln!(writer, "            <td><strong>Reasons</strong></td>")?;
        for crate_info in crates {
            write!(writer, "            <td>")?;
            if let Some(eval) = &crate_info.appraisal {
                if eval.expression_outcomes.is_empty() {
                    write!(writer, "<span class=\"na\">n/a</span>")?;
                } else {
                    for (i, outcome) in eval.expression_outcomes.iter().enumerate() {
                        if i > 0 {
                            write!(writer, "<br>")?;
                        }

                        write!(
                            writer,
                            "<span title=\"{}\">{}</span>",
                            html_escape(&outcome.description),
                            html_escape(&common::outcome_icon_name(outcome).to_string())
                        )?;
                    }
                }
            } else {
                write!(writer, "<span class=\"na\">n/a</span>")?;
            }
            writeln!(writer, "</td>")?;
        }
        writeln!(writer, "          </tr>")?;

        // Add spacer row after evaluation
        write_spacer_row(writer, crates.len())?;
    }

    // Iterate through categories in order
    let mut is_first_category = true;

    // Build per-crate metric lookup maps for O(1) access in the inner loop
    let crate_metric_maps = common::build_metric_lookup_maps(crates);

    for category in MetricCategory::iter() {
        if let Some(category_metrics) = metrics_by_category.get(&category) {
            // Add spacer row before each category except the first
            if !is_first_category {
                write_spacer_row(writer, crates.len())?;
            }
            is_first_category = false;

            // Category header row
            writeln!(writer, "          <tr class=\"category-header\">")?;
            writeln!(writer, "            <td>{category}</td>")?;

            // Empty cells for each crate column
            for _ in crates {
                writeln!(writer, "            <td></td>")?;
            }

            writeln!(writer, "          </tr>")?;

            // Metric rows for this category
            let mut tooltip_buf = String::new();
            let mut metric_buf = String::new();
            for &metric_name in category_metrics {
                writeln!(writer, "          <tr class=\"metric-row\">")?;

                // Find the metric description from the first crate
                let description = crate_metric_maps
                    .first()
                    .and_then(|map| map.get(metric_name))
                    .map_or(metric_name, |m| m.description());

                writeln!(
                    writer,
                    "            <td title=\"{}\">{}</td>",
                    html_escape(description),
                    html_escape(metric_name)
                )?;

                // Value columns for each crate
                for (crate_index, metric_map) in crate_metric_maps.iter().enumerate() {
                    let metric = metric_map.get(metric_name);
                    let crate_info = &crates[crate_index];

                    tooltip_buf.clear();
                    let _ = write!(tooltip_buf, "{} v{}\n{metric_name}\n{description}", crate_info.name, crate_info.version);

                    write!(writer, "            <td title=\"{}\"", html_escape(&tooltip_buf))?;
                    write!(writer, ">")?;
                    if let Some(m) = metric {
                        if let Some(value) = &m.value {
                            metric_buf.clear();
                            common::write_metric_value(&mut metric_buf, value);

                            // Special handling for keywords and categories
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
                    } else {
                        write!(writer, "<span class=\"na\">n/a</span>")?;
                    }
                    writeln!(writer, "</td>")?;
                }

                writeln!(writer, "          </tr>")?;
            }
        }
    }

    writeln!(writer, "        </tbody>")?;
    writeln!(writer, "      </table>")?;
    writeln!(writer, "    </div>")?;
    writeln!(writer, "  </div>")?;

    writeln!(writer, "  <script>")?;
    writeln!(writer, "    function getSystemTheme() {{")?;
    writeln!(
        writer,
        "      return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';"
    )?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    function updateIcon(theme) {{")?;
    writeln!(writer, "      const icon = document.getElementById('theme-icon');")?;
    writeln!(writer, "      if (theme === 'dark') {{")?;
    writeln!(
        writer,
        "        icon.innerHTML = '<circle cx=\"12\" cy=\"12\" r=\"4\" fill=\"currentColor\"/><path d=\"M12 1v2m0 18v2M4.22 4.22l1.42 1.42m12.72 12.72l1.42 1.42M1 12h2m18 0h2M4.22 19.78l1.42-1.42m12.72-12.72l1.42-1.42\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\"/>';"
    )?;
    writeln!(writer, "      }} else {{")?;
    writeln!(
        writer,
        "        icon.innerHTML = '<path d=\"M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z\"/>';"
    )?;
    writeln!(writer, "      }}")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    function applyTheme(theme) {{")?;
    writeln!(writer, "      document.body.classList.remove('dark-theme', 'light-theme');")?;
    writeln!(writer, "      document.body.classList.add(theme + '-theme');")?;
    writeln!(writer, "      updateIcon(theme);")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    function toggleTheme() {{")?;
    writeln!(
        writer,
        "      const currentTheme = localStorage.getItem('theme') || getSystemTheme();"
    )?;
    writeln!(writer, "      const newTheme = currentTheme === 'dark' ? 'light' : 'dark';")?;
    writeln!(writer, "      localStorage.setItem('theme', newTheme);")?;
    writeln!(writer, "      applyTheme(newTheme);")?;
    writeln!(writer, "    }}")?;
    writeln!(writer, "    const savedTheme = localStorage.getItem('theme');")?;
    writeln!(writer, "    applyTheme(savedTheme || getSystemTheme());")?;
    writeln!(writer, "  </script>")?;
    writeln!(writer, "</body>")?;
    writeln!(writer, "</html>")?;

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

fn write_spacer_row<W: Write>(writer: &mut W, crate_count: usize) -> Result<()> {
    writeln!(writer, "          <tr class=\"category-spacer\">")?;
    writeln!(writer, "            <td></td>")?;
    for _ in 0..crate_count {
        writeln!(writer, "            <td></td>")?;
    }
    writeln!(writer, "          </tr>")?;
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
        assert!(output.contains("Crate Metrics"));
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
}
