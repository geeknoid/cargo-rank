use crate::Result;
use crate::config::Config;
use crate::facts::{CrateFacts, ProviderResult};
use crate::metrics::{Metric, MetricCategory};
use crate::misc::ColorMode;
use crate::ranking::{PolicyOutcome, RankingOutcome};
use core::cmp::Ordering;
use core::fmt::{self, Write};
use owo_colors::OwoColorize;
use std::io::{IsTerminal, stdout};
use strum::IntoEnumIterator;
use terminal_size::{Width, terminal_size};

const DEFAULT_TERMINAL_WIDTH: usize = 120;
const SEPARATOR_WIDTH: usize = 40;
const TABLE_INDENT: usize = 4;
const COLUMN_GAP: usize = 2;
const MIN_DESCRIPTION_WIDTH: usize = 20;
const POINTS_WIDTH: usize = 6;

pub fn generate<W: Write>(
    crates: &[(CrateFacts, RankingOutcome)],
    config: &Config,
    color: ColorMode,
    short: bool,
    writer: &mut W,
) -> Result<()> {
    ConsoleReporter::new(writer, config, color).generate_report(crates, short)
}

struct ConsoleReporter<'a, W: Write> {
    writer: &'a mut W,
    colors: ColorScheme<'a>,
    layout: Layout,
    categories: Vec<MetricCategory>,
}

impl<'a, W: Write> ConsoleReporter<'a, W> {
    fn new(writer: &'a mut W, config: &'a Config, color_mode: ColorMode) -> Self {
        Self {
            writer,
            colors: ColorScheme::new(config, color_mode),
            layout: Layout::new(),
            categories: sorted_categories(),
        }
    }

    fn generate_report(&mut self, crates: &[(CrateFacts, RankingOutcome)], short: bool) -> Result<()> {
        let sorted = sort_by_score(crates);

        if short {
            for (facts, ranking) in &sorted {
                self.write_short_line(facts, ranking)?;
            }
        } else {
            for (index, (facts, ranking)) in sorted.iter().enumerate() {
                if index > 0 {
                    self.write_separator()?;
                }
                self.write_crate_header(facts, ranking)?;
                self.write_category_scores(ranking)?;
                self.write_metrics_table(ranking)?;
            }
        }
        Ok(())
    }

    fn write_short_line(&mut self, facts: &CrateFacts, ranking: &RankingOutcome) -> Result<()> {
        let ProviderResult::Found(crate_overall_data) = &facts.crate_overall_data else {
            unreachable!("analyzable crate must have Found data");
        };
        let ProviderResult::Found(crate_version_data) = &facts.crate_version_data else {
            unreachable!("analyzable crate must have Found data");
        };
        write!(self.writer, "{} {} ", crate_overall_data.name, crate_version_data.version)?;
        self.colors
            .write_colorized_score(self.writer, ranking.overall_score, ScoreType::Overall, None)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_separator(&mut self) -> Result<()> {
        writeln!(self.writer)?;
        self.colors
            .write_styled_line(self.writer, "═", SEPARATOR_WIDTH, TextStyle::Dimmed)?;
        writeln!(self.writer)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_crate_header(&mut self, facts: &CrateFacts, ranking: &RankingOutcome) -> Result<()> {
        let ProviderResult::Found(crate_overall_data) = &facts.crate_overall_data else {
            unreachable!("analyzable crate must have Found data");
        };
        let ProviderResult::Found(crate_version_data) = &facts.crate_version_data else {
            unreachable!("analyzable crate must have Found data");
        };

        writeln!(self.writer, "Crate          : {}", crate_overall_data.name)?;
        writeln!(self.writer, "Version        : {}", crate_version_data.version)?;
        writeln!(self.writer, "Dependency Type: {:?}", ranking.dependency_type)?;

        // Add description if available
        if let Some(description) = &crate_version_data.description {
            writeln!(self.writer, "Description    : {description}")?;
        }

        // Add categories if available
        if !crate_overall_data.categories.is_empty() {
            writeln!(self.writer, "Categories     : {}", crate_overall_data.categories.join(", "))?;
        }

        // Add keywords if available
        if !crate_overall_data.keywords.is_empty() {
            writeln!(self.writer, "Keywords       : {}", crate_overall_data.keywords.join(", "))?;
        }

        write!(self.writer, "Overall Score  : ")?;
        self.colors
            .write_colorized_score(self.writer, ranking.overall_score, ScoreType::Overall, None)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_category_scores(&mut self, ranking: &RankingOutcome) -> Result<()> {
        writeln!(self.writer)?;
        self.colors.write_styled_text(self.writer, "Category Scores", TextStyle::Bold)?;
        writeln!(self.writer)?;

        let num_categories = self.categories.len();
        for i in 0..num_categories {
            let category = self.categories[i];
            write!(self.writer, "  {category:<width$}: ", width = self.layout.category_width)?;
            self.write_category_score(ranking, category, Some(3))?;
            writeln!(self.writer)?;
        }
        Ok(())
    }

    fn write_metrics_table(&mut self, ranking: &RankingOutcome) -> Result<()> {
        let table = Table::new(&self.layout, ranking);

        self.write_table_header(&table)?;

        let num_categories = self.categories.len();
        for index in 0..num_categories {
            let category = self.categories[index];
            self.write_category_row(ranking, category)?;
            self.write_metric_rows(ranking, category, &table)?;

            if index < num_categories - 1 {
                writeln!(self.writer)?;
            }
        }
        Ok(())
    }

    fn write_table_header(&mut self, table: &Table) -> Result<()> {
        writeln!(self.writer)?;
        self.colors.write_styled_line(self.writer, "─", table.width, TextStyle::Dimmed)?;
        writeln!(self.writer)?;

        write!(self.writer, "    ")?;
        self.colors.write_styled_text(self.writer, "Metric", TextStyle::Bold)?;
        write!(self.writer, "{:width$}  ", "", width = self.layout.metric_width - "Metric".len())?;
        self.colors.write_styled_text(self.writer, "Points", TextStyle::Bold)?;
        write!(self.writer, "{:width$}  ", "", width = POINTS_WIDTH - "Points".len())?;
        self.colors.write_styled_text(self.writer, "Notes", TextStyle::Bold)?;
        writeln!(self.writer)?;

        self.colors.write_styled_line(self.writer, "─", table.width, TextStyle::Dimmed)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_category_row(&mut self, ranking: &RankingOutcome, category: MetricCategory) -> Result<()> {
        write!(self.writer, "{category:?} = ")?;
        self.write_category_score(ranking, category, None)?;
        writeln!(self.writer)?;
        Ok(())
    }

    fn write_metric_rows(&mut self, ranking: &RankingOutcome, category: MetricCategory, table: &Table) -> Result<()> {
        for metric in Metric::iter().filter(|m| m.category() == category) {
            let (points, description) = extract_outcome(ranking, metric);
            let truncated = truncate(description, table.max_description_width);

            let metric_width = self.layout.metric_width;
            write!(self.writer, "    {metric:<metric_width$}  ")?;
            match points {
                Some(p) => {
                    let points_width = POINTS_WIDTH;
                    write!(self.writer, "{p:>points_width$}  ")?;
                }
                None => write!(self.writer, "{:>POINTS_WIDTH$}  ", "-")?,
            }
            writeln!(self.writer, "{truncated}")?;
        }
        Ok(())
    }

    fn write_category_score(&mut self, ranking: &RankingOutcome, category: MetricCategory, padding: Option<usize>) -> Result<()> {
        match ranking.category_scores.get(&category) {
            None => {
                if padding.is_some() {
                    write!(self.writer, "  -")?;
                } else {
                    write!(self.writer, "-")?;
                }
            }
            Some(&score) => {
                self.colors
                    .write_colorized_score(self.writer, score, ScoreType::Category(category), padding)?;
            }
        }
        Ok(())
    }
}

#[derive(Copy, Clone)]
enum TextStyle {
    Bold,
    Dimmed,
}

#[derive(Copy, Clone)]
enum ScoreType {
    Overall,
    Category(MetricCategory),
}

struct ColorScheme<'a> {
    config: &'a Config,
    enabled: bool,
}

impl<'a> ColorScheme<'a> {
    fn new(config: &'a Config, color_mode: ColorMode) -> Self {
        let enabled = matches!(color_mode, ColorMode::Always) || (matches!(color_mode, ColorMode::Auto) && stdout().is_terminal());
        Self { config, enabled }
    }

    fn write_styled_text<W: Write>(&self, writer: &mut W, text: &str, style: TextStyle) -> fmt::Result {
        if !self.enabled {
            return write!(writer, "{text}");
        }
        match style {
            TextStyle::Bold => write!(writer, "{}", text.bold()),
            TextStyle::Dimmed => write!(writer, "{}", text.dimmed()),
        }
    }

    fn write_styled_line<W: Write>(&self, writer: &mut W, ch: &str, width: usize, style: TextStyle) -> fmt::Result {
        if !self.enabled {
            return write!(writer, "{}", ch.repeat(width));
        }
        match style {
            TextStyle::Bold => write!(writer, "{}", ch.repeat(width).bold()),
            TextStyle::Dimmed => write!(writer, "{}", ch.repeat(width).dimmed()),
        }
    }

    fn write_colorized_score<W: Write>(&self, writer: &mut W, score: f64, score_type: ScoreType, padding: Option<usize>) -> fmt::Result {
        #[expect(clippy::cast_possible_truncation, reason = "Intentional rounding for display")]
        let score_int = score.round() as i32;

        if !self.enabled {
            return match padding {
                Some(width) => write!(writer, "{score_int:>width$}"),
                None => write!(writer, "{score_int}"),
            };
        }

        let color_index = match score_type {
            ScoreType::Overall => self.config.color_index_for_score(score),
            ScoreType::Category(cat) => self.config.color_index_for_category_score(cat, score),
        };

        match color_index {
            None => match padding {
                Some(width) => write!(writer, "{}", format!("{score_int:>width$}").dimmed()),
                None => write!(writer, "{}", score_int.dimmed()),
            },
            Some(idx) => {
                let color = self.config.colors_for_scoring_bands[idx].0;
                match padding {
                    Some(width) => write!(
                        writer,
                        "{}",
                        format!("{score_int:>width$}").truecolor(color.red, color.green, color.blue)
                    ),
                    None => write!(writer, "{}", score_int.truecolor(color.red, color.green, color.blue)),
                }
            }
        }
    }
}

#[expect(clippy::struct_field_names, reason = "Width suffix clearly indicates dimension measurements")]
struct Layout {
    terminal_width: usize,
    category_width: usize,
    metric_width: usize,
}

impl Layout {
    fn new() -> Self {
        let terminal_width = detect_terminal_width();
        Self {
            terminal_width,
            category_width: MetricCategory::iter().map(|c| format!("{c:?}").len()).max().unwrap_or(18),
            metric_width: Metric::iter().map(|m| format!("{m}").len()).max().unwrap_or(35),
        }
    }
}

struct Table {
    width: usize,
    max_description_width: usize,
}

impl Table {
    fn new(layout: &Layout, ranking: &RankingOutcome) -> Self {
        let max_description_width = layout
            .terminal_width
            .saturating_sub(TABLE_INDENT + layout.metric_width + COLUMN_GAP + POINTS_WIDTH + COLUMN_GAP)
            .max(MIN_DESCRIPTION_WIDTH);

        let actual_width = Metric::iter()
            .map(|m| truncate(extract_outcome(ranking, m).1, max_description_width).len())
            .max()
            .unwrap_or(0)
            .max("Notes".len());

        Self {
            width: TABLE_INDENT + layout.metric_width + COLUMN_GAP + POINTS_WIDTH + COLUMN_GAP + actual_width,
            max_description_width,
        }
    }
}

#[expect(clippy::cast_possible_truncation, reason = "Intentional rounding for display")]
fn extract_outcome(ranking: &RankingOutcome, metric: Metric) -> (Option<i32>, &str) {
    ranking.details.get(&metric).map_or_else(
        || (None, "n/a"),
        |outcome| match outcome {
            PolicyOutcome::Match(points, info) => (Some(points.round() as i32), info.as_str()),
            PolicyOutcome::NoMatch(reason) => (Some(0), reason.as_str()),
        },
    )
}

fn truncate(text: &str, max_width: usize) -> String {
    if text.len() <= max_width {
        return text.to_string();
    }

    let mut result = String::new();
    for ch in text.chars() {
        if result.len() + 1 >= max_width {
            break;
        }
        result.push(ch);
    }

    format!("{result}…")
}

fn detect_terminal_width() -> usize {
    if stdout().is_terminal() {
        terminal_size().map_or(DEFAULT_TERMINAL_WIDTH, |(Width(w), _)| usize::from(w))
    } else {
        DEFAULT_TERMINAL_WIDTH
    }
}

fn sorted_categories() -> Vec<MetricCategory> {
    let mut categories: Vec<_> = MetricCategory::iter().collect();
    categories.sort_by_key(|c| format!("{c:?}"));
    categories
}

fn sort_by_score(crates: &[(CrateFacts, RankingOutcome)]) -> Vec<&(CrateFacts, RankingOutcome)> {
    let mut sorted: Vec<_> = crates.iter().collect();
    sorted.sort_unstable_by(|a, b| a.1.overall_score.partial_cmp(&b.1.overall_score).unwrap_or(Ordering::Equal));
    sorted
}
