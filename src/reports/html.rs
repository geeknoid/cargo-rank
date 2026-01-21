use crate::Result;
use crate::config::Config;
use crate::facts::{CrateFacts, ProviderResult};
use crate::metrics::{Metric, MetricCategory};
use crate::misc::DependencyType;
use crate::ranking::{PolicyOutcome, RankingOutcome};
use core::cmp::Ordering;
use core::fmt::Write;
use std::collections::HashMap;
use strum::IntoEnumIterator;

pub fn generate<W: Write>(crates: &[(CrateFacts, RankingOutcome)], config: &Config, title: &str, writer: &mut W) -> Result<()> {
    let mut generator = HtmlGenerator::new(writer, config, title);
    generator.generate(crates)
}

struct HtmlGenerator<'a, W: Write> {
    writer: &'a mut W,
    config: &'a Config,
    title: &'a str,
}

impl<'a, W: Write> HtmlGenerator<'a, W> {
    const fn new(writer: &'a mut W, config: &'a Config, title: &'a str) -> Self {
        Self { writer, config, title }
    }

    fn generate(&mut self, crates: &[(CrateFacts, RankingOutcome)]) -> Result<()> {
        // Group crates by dependency type
        let mut by_dep_type: HashMap<DependencyType, Vec<&(CrateFacts, RankingOutcome)>> = HashMap::new();
        for item in crates {
            by_dep_type.entry(item.1.dependency_type).or_default().push(item);
        }

        // Sort each group by score
        for items in by_dep_type.values_mut() {
            items.sort_by(|a, b| b.1.overall_score.partial_cmp(&a.1.overall_score).unwrap_or(Ordering::Equal));
        }

        // Get sorted dependency types
        let mut dep_types: Vec<_> = by_dep_type.keys().copied().collect();
        dep_types.sort_by_key(|dt| format!("{dt:?}"));

        self.write_header()?;
        self.write_styles()?;
        self.write_body_start()?;
        self.write_title()?;

        if dep_types.len() > 1 {
            self.write_tabs(&dep_types)?;
        }

        for (idx, dep_type) in dep_types.iter().enumerate() {
            if let Some(items) = by_dep_type.get(dep_type) {
                self.write_dependency_section(*dep_type, items, idx == 0, dep_types.len() > 1)?;
            }
        }

        self.write_script(&dep_types)?;
        self.write_footer()?;
        Ok(())
    }

    fn write_header(&mut self) -> Result<()> {
        writeln!(self.writer, "<!DOCTYPE html>")?;
        writeln!(self.writer, "<html lang=\"en\">")?;
        writeln!(self.writer, "<head>")?;
        writeln!(self.writer, "<meta charset=\"UTF-8\">")?;
        writeln!(
            self.writer,
            "<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">"
        )?;
        writeln!(self.writer, "<title>{}</title>", html_escape(self.title))?;
        Ok(())
    }

    #[expect(clippy::too_many_lines, reason = "CSS generation requires many lines")]
    fn write_styles(&mut self) -> Result<()> {
        writeln!(self.writer, "<style>")?;
        writeln!(self.writer, ":root {{")?;
        writeln!(self.writer, "  color-scheme: light dark;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, "@media (prefers-color-scheme: light) {{")?;
        writeln!(self.writer, "  :root {{")?;
        writeln!(self.writer, "    --bg-primary: #ffffff;")?;
        writeln!(self.writer, "    --bg-secondary: #f5f5f5;")?;
        writeln!(self.writer, "    --bg-hover: #e8e8e8;")?;
        writeln!(self.writer, "    --text-primary: #1a1a1a;")?;
        writeln!(self.writer, "    --text-secondary: #666666;")?;
        writeln!(self.writer, "    --border-color: #e0e0e0;")?;
        writeln!(self.writer, "    --tab-active: #007acc;")?;
        writeln!(self.writer, "    --shadow: rgba(0, 0, 0, 0.1);")?;
        writeln!(self.writer, "  }}")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, "@media (prefers-color-scheme: dark) {{")?;
        writeln!(self.writer, "  :root {{")?;
        writeln!(self.writer, "    --bg-primary: #1e1e1e;")?;
        writeln!(self.writer, "    --bg-secondary: #2d2d2d;")?;
        writeln!(self.writer, "    --bg-hover: #3a3a3a;")?;
        writeln!(self.writer, "    --text-primary: #e0e0e0;")?;
        writeln!(self.writer, "    --text-secondary: #b0b0b0;")?;
        writeln!(self.writer, "    --border-color: #404040;")?;
        writeln!(self.writer, "    --tab-active: #0098ff;")?;
        writeln!(self.writer, "    --shadow: rgba(0, 0, 0, 0.3);")?;
        writeln!(self.writer, "  }}")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, "* {{")?;
        writeln!(self.writer, "  margin: 0;")?;
        writeln!(self.writer, "  padding: 0;")?;
        writeln!(self.writer, "  box-sizing: border-box;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, "body {{")?;
        writeln!(
            self.writer,
            "  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Helvetica Neue', Arial, sans-serif;"
        )?;
        writeln!(self.writer, "  background: var(--bg-primary);")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "  line-height: 1.6;")?;
        writeln!(self.writer, "  padding: 20px;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".container {{")?;
        writeln!(self.writer, "  max-width: 1400px;")?;
        writeln!(self.writer, "  margin: 0 auto;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, "h1 {{")?;
        writeln!(self.writer, "  font-size: 2.5rem;")?;
        writeln!(self.writer, "  margin-bottom: 2rem;")?;
        writeln!(self.writer, "  text-align: center;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".tabs {{")?;
        writeln!(self.writer, "  display: flex;")?;
        writeln!(self.writer, "  gap: 10px;")?;
        writeln!(self.writer, "  margin-bottom: 30px;")?;
        writeln!(self.writer, "  border-bottom: 2px solid var(--border-color);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".tab {{")?;
        writeln!(self.writer, "  padding: 12px 24px;")?;
        writeln!(self.writer, "  cursor: pointer;")?;
        writeln!(self.writer, "  background: transparent;")?;
        writeln!(self.writer, "  border: none;")?;
        writeln!(self.writer, "  color: var(--text-secondary);")?;
        writeln!(self.writer, "  font-size: 1rem;")?;
        writeln!(self.writer, "  font-weight: 500;")?;
        writeln!(self.writer, "  border-bottom: 3px solid transparent;")?;
        writeln!(self.writer, "  margin-bottom: -2px;")?;
        writeln!(self.writer, "  transition: all 0.2s ease;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".tab:hover {{")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".tab.active {{")?;
        writeln!(self.writer, "  color: var(--tab-active);")?;
        writeln!(self.writer, "  border-bottom-color: var(--tab-active);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".dep-section {{")?;
        writeln!(self.writer, "  display: none;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".dep-section.active {{")?;
        writeln!(self.writer, "  display: block;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-summary {{")?;
        writeln!(self.writer, "  background: var(--bg-secondary);")?;
        writeln!(self.writer, "  border-radius: 8px;")?;
        writeln!(self.writer, "  padding: 20px;")?;
        writeln!(self.writer, "  margin-bottom: 20px;")?;
        writeln!(self.writer, "  box-shadow: 0 2px 4px var(--shadow);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-summary h2 {{")?;
        writeln!(self.writer, "  font-size: 1.5rem;")?;
        writeln!(self.writer, "  margin-bottom: 15px;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-list {{")?;
        writeln!(self.writer, "  display: grid;")?;
        writeln!(self.writer, "  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));")?;
        writeln!(self.writer, "  gap: 15px;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-item {{")?;
        writeln!(self.writer, "  background: var(--bg-primary);")?;
        writeln!(self.writer, "  border: 1px solid var(--border-color);")?;
        writeln!(self.writer, "  border-radius: 6px;")?;
        writeln!(self.writer, "  padding: 15px;")?;
        writeln!(self.writer, "  cursor: pointer;")?;
        writeln!(self.writer, "  transition: all 0.2s ease;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-item:hover {{")?;
        writeln!(self.writer, "  background: var(--bg-hover);")?;
        writeln!(self.writer, "  transform: translateY(-2px);")?;
        writeln!(self.writer, "  box-shadow: 0 4px 8px var(--shadow);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-name {{")?;
        writeln!(self.writer, "  font-weight: 600;")?;
        writeln!(self.writer, "  font-size: 1.1rem;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "  margin-bottom: 5px;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-version {{")?;
        writeln!(self.writer, "  color: var(--text-secondary);")?;
        writeln!(self.writer, "  font-size: 0.9rem;")?;
        writeln!(self.writer, "  margin-bottom: 8px;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-score {{")?;
        writeln!(self.writer, "  font-size: 1.5rem;")?;
        writeln!(self.writer, "  font-weight: 700;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-details {{")?;
        writeln!(self.writer, "  background: var(--bg-secondary);")?;
        writeln!(self.writer, "  border-radius: 8px;")?;
        writeln!(self.writer, "  padding: 30px;")?;
        writeln!(self.writer, "  margin-bottom: 30px;")?;
        writeln!(self.writer, "  box-shadow: 0 2px 8px var(--shadow);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-header {{")?;
        writeln!(self.writer, "  margin-bottom: 25px;")?;
        writeln!(self.writer, "  padding-bottom: 20px;")?;
        writeln!(self.writer, "  border-bottom: 2px solid var(--border-color);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-header h3 {{")?;
        writeln!(self.writer, "  font-size: 2rem;")?;
        writeln!(self.writer, "  margin-bottom: 15px;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".header-info {{")?;
        writeln!(self.writer, "  display: grid;")?;
        writeln!(self.writer, "  grid-template-columns: auto 1fr;")?;
        writeln!(self.writer, "  gap: 10px 20px;")?;
        writeln!(self.writer, "  font-size: 1rem;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".header-label {{")?;
        writeln!(self.writer, "  font-weight: 600;")?;
        writeln!(self.writer, "  color: var(--text-secondary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".header-value {{")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-description {{")?;
        writeln!(self.writer, "  margin-top: 15px;")?;
        writeln!(self.writer, "  padding-top: 15px;")?;
        writeln!(self.writer, "  border-top: 1px solid var(--border-color);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-description p {{")?;
        writeln!(self.writer, "  margin-top: 8px;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "  line-height: 1.5;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".crate-categories, .crate-keywords {{")?;
        writeln!(self.writer, "  margin-top: 15px;")?;
        writeln!(self.writer, "  padding-top: 15px;")?;
        writeln!(self.writer, "  border-top: 1px solid var(--border-color);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".tag-list {{")?;
        writeln!(self.writer, "  display: flex;")?;
        writeln!(self.writer, "  flex-wrap: wrap;")?;
        writeln!(self.writer, "  gap: 8px;")?;
        writeln!(self.writer, "  margin-top: 8px;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".tag {{")?;
        writeln!(self.writer, "  display: inline-block;")?;
        writeln!(self.writer, "  background: var(--bg-primary);")?;
        writeln!(self.writer, "  border: 1px solid var(--border-color);")?;
        writeln!(self.writer, "  border-radius: 4px;")?;
        writeln!(self.writer, "  padding: 4px 10px;")?;
        writeln!(self.writer, "  font-size: 0.85rem;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".category-scores {{")?;
        writeln!(self.writer, "  margin-bottom: 30px;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".category-scores h4 {{")?;
        writeln!(self.writer, "  font-size: 1.3rem;")?;
        writeln!(self.writer, "  margin-bottom: 15px;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".category-grid {{")?;
        writeln!(self.writer, "  display: grid;")?;
        writeln!(self.writer, "  grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));")?;
        writeln!(self.writer, "  gap: 15px;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".category-item {{")?;
        writeln!(self.writer, "  background: var(--bg-primary);")?;
        writeln!(self.writer, "  border: 1px solid var(--border-color);")?;
        writeln!(self.writer, "  border-radius: 6px;")?;
        writeln!(self.writer, "  padding: 12px;")?;
        writeln!(self.writer, "  display: flex;")?;
        writeln!(self.writer, "  justify-content: space-between;")?;
        writeln!(self.writer, "  align-items: center;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".category-name {{")?;
        writeln!(self.writer, "  font-weight: 500;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".category-score {{")?;
        writeln!(self.writer, "  font-size: 1.2rem;")?;
        writeln!(self.writer, "  font-weight: 700;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".metrics-table {{")?;
        writeln!(self.writer, "  margin-top: 20px;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".metrics-table h4 {{")?;
        writeln!(self.writer, "  font-size: 1.3rem;")?;
        writeln!(self.writer, "  margin-bottom: 15px;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".metric-category {{")?;
        writeln!(self.writer, "  margin-bottom: 25px;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".metric-category-header {{")?;
        writeln!(self.writer, "  font-size: 1.1rem;")?;
        writeln!(self.writer, "  font-weight: 600;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "  margin-bottom: 10px;")?;
        writeln!(self.writer, "  padding: 8px 0;")?;
        writeln!(self.writer, "  border-bottom: 1px solid var(--border-color);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".metric-row {{")?;
        writeln!(self.writer, "  display: grid;")?;
        writeln!(self.writer, "  grid-template-columns: 2fr 1fr 3fr;")?;
        writeln!(self.writer, "  gap: 15px;")?;
        writeln!(self.writer, "  padding: 10px;")?;
        writeln!(self.writer, "  border-bottom: 1px solid var(--border-color);")?;
        writeln!(self.writer, "  align-items: center;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".metric-row:last-child {{")?;
        writeln!(self.writer, "  border-bottom: none;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".metric-name {{")?;
        writeln!(self.writer, "  font-weight: 500;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".metric-points {{")?;
        writeln!(self.writer, "  text-align: right;")?;
        writeln!(self.writer, "  font-weight: 600;")?;
        writeln!(self.writer, "  color: var(--text-primary);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".metric-notes {{")?;
        writeln!(self.writer, "  color: var(--text-secondary);")?;
        writeln!(self.writer, "  font-size: 0.95rem;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".back-to-top {{")?;
        writeln!(self.writer, "  position: fixed;")?;
        writeln!(self.writer, "  bottom: 30px;")?;
        writeln!(self.writer, "  right: 30px;")?;
        writeln!(self.writer, "  background: var(--tab-active);")?;
        writeln!(self.writer, "  color: white;")?;
        writeln!(self.writer, "  border: none;")?;
        writeln!(self.writer, "  border-radius: 50%;")?;
        writeln!(self.writer, "  width: 50px;")?;
        writeln!(self.writer, "  height: 50px;")?;
        writeln!(self.writer, "  font-size: 1.5rem;")?;
        writeln!(self.writer, "  cursor: pointer;")?;
        writeln!(self.writer, "  box-shadow: 0 4px 8px var(--shadow);")?;
        writeln!(self.writer, "  transition: all 0.3s ease;")?;
        writeln!(self.writer, "  display: none;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".back-to-top:hover {{")?;
        writeln!(self.writer, "  transform: translateY(-3px);")?;
        writeln!(self.writer, "  box-shadow: 0 6px 12px var(--shadow);")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, ".back-to-top.visible {{")?;
        writeln!(self.writer, "  display: block;")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, "</style>")?;
        Ok(())
    }

    fn write_body_start(&mut self) -> Result<()> {
        writeln!(self.writer, "</head>")?;
        writeln!(self.writer, "<body>")?;
        writeln!(self.writer, "<div class=\"container\">")?;
        Ok(())
    }

    fn write_title(&mut self) -> Result<()> {
        writeln!(self.writer, "<h1>{}</h1>", html_escape(self.title))?;
        Ok(())
    }

    fn write_tabs(&mut self, dep_types: &[DependencyType]) -> Result<()> {
        writeln!(self.writer, "<div class=\"tabs\">")?;
        for (idx, dep_type) in dep_types.iter().enumerate() {
            let active = if idx == 0 { " active" } else { "" };
            writeln!(
                self.writer,
                "<button class=\"tab{active}\" onclick=\"switchTab('{dep_type:?}')\">{dep_type:?}</button>"
            )?;
        }
        writeln!(self.writer, "</div>")?;
        Ok(())
    }

    fn write_dependency_section(
        &mut self,
        dep_type: DependencyType,
        items: &[&(CrateFacts, RankingOutcome)],
        is_active: bool,
        has_tabs: bool,
    ) -> Result<()> {
        let active_class = if is_active { " active" } else { "" };
        writeln!(self.writer, "<div class=\"dep-section{active_class}\" id=\"section-{dep_type:?}\">")?;

        // Summary section
        writeln!(self.writer, "<div class=\"crate-summary\">")?;
        if has_tabs {
            writeln!(self.writer, "<h2>{dep_type:?} Dependencies</h2>")?;
        } else {
            writeln!(self.writer, "<h2>Analyzed Crates</h2>")?;
        }

        writeln!(self.writer, "<div class=\"crate-list\">")?;
        for (facts, ranking) in items {
            let ProviderResult::Found(crate_overall_data) = &facts.crate_overall_data else {
                unreachable!("analyzable crate must have Found data");
            };
            let ProviderResult::Found(crate_version_data) = &facts.crate_version_data else {
                unreachable!("analyzable crate must have Found data");
            };
            let crate_id = format!("{}_{:?}", crate_overall_data.name, dep_type);
            writeln!(
                self.writer,
                "<div class=\"crate-item\" onclick=\"scrollToCrate('{crate_id}')\" role=\"button\" tabindex=\"0\">"
            )?;
            writeln!(
                self.writer,
                "<div class=\"crate-name\">{}</div>",
                html_escape(&crate_overall_data.name)
            )?;
            writeln!(self.writer, "<div class=\"crate-version\">v{}</div>", crate_version_data.version)?;

            #[expect(clippy::cast_possible_truncation, reason = "Intentional rounding for display")]
            let score = ranking.overall_score.round() as i32;
            let color = self.get_score_color(ranking.overall_score);
            writeln!(self.writer, "<div class=\"crate-score\" style=\"color: {color}\">{score}</div>")?;
            writeln!(self.writer, "</div>")?;
        }
        writeln!(self.writer, "</div>")?; // crate-list
        writeln!(self.writer, "</div>")?; // crate-summary

        // Detailed sections
        for (facts, ranking) in items {
            let ProviderResult::Found(crate_overall_data) = &facts.crate_overall_data else {
                unreachable!("analyzable crate must have Found data");
            };
            let crate_id = format!("{}_{:?}", crate_overall_data.name, dep_type);
            self.write_crate_details(&crate_id, facts, ranking)?;
        }

        writeln!(self.writer, "</div>")?; // dep-section
        Ok(())
    }

    fn write_crate_details(&mut self, crate_id: &str, facts: &CrateFacts, ranking: &RankingOutcome) -> Result<()> {
        let ProviderResult::Found(crate_overall_data) = &facts.crate_overall_data else {
            unreachable!("analyzable crate must have Found data");
        };
        let ProviderResult::Found(crate_version_data) = &facts.crate_version_data else {
            unreachable!("analyzable crate must have Found data");
        };

        writeln!(self.writer, "<div class=\"crate-details\" id=\"{crate_id}\">")?;

        // Header
        writeln!(self.writer, "<div class=\"crate-header\">")?;
        writeln!(self.writer, "<h3>{}</h3>", html_escape(&crate_overall_data.name))?;
        writeln!(self.writer, "<div class=\"header-info\">")?;

        writeln!(self.writer, "<span class=\"header-label\">Version:</span>")?;
        writeln!(self.writer, "<span class=\"header-value\">{}</span>", crate_version_data.version)?;

        writeln!(self.writer, "<span class=\"header-label\">Dependency Type:</span>")?;
        writeln!(self.writer, "<span class=\"header-value\">{:?}</span>", ranking.dependency_type)?;

        writeln!(self.writer, "<span class=\"header-label\">Overall Score:</span>")?;
        #[expect(clippy::cast_possible_truncation, reason = "Intentional rounding for display")]
        let score = ranking.overall_score.round() as i32;
        let color = self.get_score_color(ranking.overall_score);
        writeln!(
            self.writer,
            "<span class=\"header-value\" style=\"color: {color}; font-weight: 700; font-size: 1.2rem;\">{score}</span>"
        )?;

        writeln!(self.writer, "</div>")?; // header-info

        // Add description if available
        if let Some(description) = &crate_version_data.description {
            writeln!(self.writer, "<div class=\"crate-description\">")?;
            writeln!(self.writer, "<span class=\"header-label\">Description:</span>")?;
            writeln!(self.writer, "<p>{}</p>", html_escape(description))?;
            writeln!(self.writer, "</div>")?;
        }

        // Add categories if available
        if !crate_overall_data.categories.is_empty() {
            writeln!(self.writer, "<div class=\"crate-categories\">")?;
            writeln!(self.writer, "<span class=\"header-label\">Categories:</span>")?;
            writeln!(self.writer, "<div class=\"tag-list\">")?;
            for category in &crate_overall_data.categories {
                writeln!(self.writer, "<span class=\"tag\">{}</span>", html_escape(category))?;
            }
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
        }

        // Add keywords if available
        if !crate_overall_data.keywords.is_empty() {
            writeln!(self.writer, "<div class=\"crate-keywords\">")?;
            writeln!(self.writer, "<span class=\"header-label\">Keywords:</span>")?;
            writeln!(self.writer, "<div class=\"tag-list\">")?;
            for keyword in &crate_overall_data.keywords {
                writeln!(self.writer, "<span class=\"tag\">{}</span>", html_escape(keyword))?;
            }
            writeln!(self.writer, "</div>")?;
            writeln!(self.writer, "</div>")?;
        }

        writeln!(self.writer, "</div>")?; // crate-header

        // Category scores
        self.write_category_scores(ranking)?;

        // Metrics table
        self.write_metrics_table(ranking)?;

        writeln!(self.writer, "</div>")?; // crate-details
        Ok(())
    }

    fn write_category_scores(&mut self, ranking: &RankingOutcome) -> Result<()> {
        writeln!(self.writer, "<div class=\"category-scores\">")?;
        writeln!(self.writer, "<h4>Category Scores</h4>")?;
        writeln!(self.writer, "<div class=\"category-grid\">")?;

        let mut categories: Vec<_> = MetricCategory::iter().collect();
        categories.sort_by_key(|c| format!("{c:?}"));

        for category in categories {
            writeln!(self.writer, "<div class=\"category-item\">")?;
            writeln!(self.writer, "<span class=\"category-name\">{category:?}</span>")?;

            if let Some(&score) = ranking.category_scores.get(&category) {
                #[expect(clippy::cast_possible_truncation, reason = "Intentional rounding for display")]
                let score_int = score.round() as i32;
                let color = self.get_category_score_color(category, score);
                writeln!(
                    self.writer,
                    "<span class=\"category-score\" style=\"color: {color}\">{score_int}</span>"
                )?;
            } else {
                writeln!(self.writer, "<span class=\"category-score\">-</span>")?;
            }

            writeln!(self.writer, "</div>")?;
        }

        writeln!(self.writer, "</div>")?; // category-grid
        writeln!(self.writer, "</div>")?; // category-scores
        Ok(())
    }

    fn write_metrics_table(&mut self, ranking: &RankingOutcome) -> Result<()> {
        writeln!(self.writer, "<div class=\"metrics-table\">")?;
        writeln!(self.writer, "<h4>Metrics</h4>")?;

        let mut categories: Vec<_> = MetricCategory::iter().collect();
        categories.sort_by_key(|c| format!("{c:?}"));

        for category in categories {
            writeln!(self.writer, "<div class=\"metric-category\">")?;
            writeln!(self.writer, "<div class=\"metric-category-header\">{category:?}</div>")?;

            for metric in Metric::iter().filter(|m| m.category() == category) {
                let (points, description) = Self::extract_outcome(ranking, metric);

                writeln!(self.writer, "<div class=\"metric-row\">")?;
                writeln!(self.writer, "<div class=\"metric-name\">{metric}</div>")?;

                if let Some(p) = points {
                    writeln!(self.writer, "<div class=\"metric-points\">{p}</div>")?;
                } else {
                    writeln!(self.writer, "<div class=\"metric-points\">-</div>")?;
                }

                writeln!(self.writer, "<div class=\"metric-notes\">{}</div>", html_escape(&description))?;
                writeln!(self.writer, "</div>")?; // metric-row
            }

            writeln!(self.writer, "</div>")?; // metric-category
        }

        writeln!(self.writer, "</div>")?; // metrics-table
        Ok(())
    }

    fn write_script(&mut self, dep_types: &[DependencyType]) -> Result<()> {
        writeln!(
            self.writer,
            "<button class=\"back-to-top\" onclick=\"scrollToTop()\" id=\"backToTop\">↑</button>"
        )?;
        writeln!(self.writer, "<script>")?;

        if dep_types.len() > 1 {
            writeln!(self.writer, "function switchTab(depType) {{")?;
            writeln!(self.writer, "  const tabs = document.querySelectorAll('.tab');")?;
            writeln!(self.writer, "  const sections = document.querySelectorAll('.dep-section');")?;
            writeln!(self.writer, "  ")?;
            writeln!(self.writer, "  tabs.forEach(tab => tab.classList.remove('active'));")?;
            writeln!(self.writer, "  sections.forEach(section => section.classList.remove('active'));")?;
            writeln!(self.writer, "  ")?;
            writeln!(self.writer, "  event.target.classList.add('active');")?;
            writeln!(
                self.writer,
                "  document.getElementById('section-' + depType).classList.add('active');"
            )?;
            writeln!(self.writer, "  ")?;
            writeln!(self.writer, "  window.scrollTo({{ top: 0, behavior: 'smooth' }});")?;
            writeln!(self.writer, "}}")?;
            writeln!(self.writer)?;
        }

        writeln!(self.writer, "function scrollToCrate(crateId) {{")?;
        writeln!(self.writer, "  const element = document.getElementById(crateId);")?;
        writeln!(self.writer, "  if (element) {{")?;
        writeln!(self.writer, "    element.scrollIntoView({{ behavior: 'smooth', block: 'start' }});")?;
        writeln!(self.writer, "  }}")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, "function scrollToTop() {{")?;
        writeln!(self.writer, "  window.scrollTo({{ top: 0, behavior: 'smooth' }});")?;
        writeln!(self.writer, "}}")?;
        writeln!(self.writer)?;

        writeln!(self.writer, "window.addEventListener('scroll', function() {{")?;
        writeln!(self.writer, "  const backToTop = document.getElementById('backToTop');")?;
        writeln!(self.writer, "  if (window.pageYOffset > 300) {{")?;
        writeln!(self.writer, "    backToTop.classList.add('visible');")?;
        writeln!(self.writer, "  }} else {{")?;
        writeln!(self.writer, "    backToTop.classList.remove('visible');")?;
        writeln!(self.writer, "  }}")?;
        writeln!(self.writer, "}});")?;
        writeln!(self.writer)?;

        writeln!(self.writer, "document.querySelectorAll('.crate-item').forEach(item => {{")?;
        writeln!(self.writer, "  item.addEventListener('keypress', function(e) {{")?;
        writeln!(self.writer, "    if (e.key === 'Enter' || e.key === ' ') {{")?;
        writeln!(self.writer, "      e.preventDefault();")?;
        writeln!(self.writer, "      this.click();")?;
        writeln!(self.writer, "    }}")?;
        writeln!(self.writer, "  }});")?;
        writeln!(self.writer, "}});")?;

        writeln!(self.writer, "</script>")?;
        Ok(())
    }

    fn write_footer(&mut self) -> Result<()> {
        writeln!(self.writer, "</div>")?; // container
        writeln!(self.writer, "</body>")?;
        writeln!(self.writer, "</html>")?;
        Ok(())
    }

    fn get_score_color(&self, score: f64) -> String {
        match self.config.color_index_for_score(score) {
            Some(0) => "#ff4444".to_string(),
            Some(1) => "#ff8800".to_string(),
            Some(2) => "#00cc00".to_string(),
            _ => "var(--text-secondary)".to_string(),
        }
    }

    fn get_category_score_color(&self, category: MetricCategory, score: f64) -> String {
        match self.config.color_index_for_category_score(category, score) {
            Some(0) => "#ff4444".to_string(),
            Some(1) => "#ff8800".to_string(),
            Some(2) => "#00cc00".to_string(),
            _ => "var(--text-secondary)".to_string(),
        }
    }

    #[expect(clippy::cast_possible_truncation, reason = "Intentional rounding for display")]
    fn extract_outcome(ranking: &RankingOutcome, metric: Metric) -> (Option<i32>, String) {
        ranking.details.get(&metric).map_or_else(
            || (None, "n/a".to_string()),
            |outcome| match outcome {
                PolicyOutcome::Match(points, info) => (Some(points.round() as i32), info.clone()),
                PolicyOutcome::NoMatch(reason) => (Some(0), reason.clone()),
            },
        )
    }
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
