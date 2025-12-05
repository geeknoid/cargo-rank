use crate::Result;
use crate::config::Color;
use crate::config::policies::{
    AgePolicy, AgedCountPolicy, BooleanPolicy, LicensePolicy, MaxCountPolicy, MinCountPolicy, PercentagePolicy, ResponsivenessPolicy,
    VersionPolicy,
};
use crate::config::policy::Policy;
use crate::metrics::{Metric, MetricCategory};
use camino::{Utf8Path, Utf8PathBuf};
use ohno::{IntoAppError, app_err};
use palette::Srgb;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io;

/// The default configuration YAML content, embedded from `default_config.yml`
pub const DEFAULT_CONFIG_YAML: &str = include_str!("../../default_config.yml");

/// Number of scoring bands
pub const NUM_SCORING_BANDS: usize = 3;

/// Default overall scoring thresholds: `[orange_threshold, green_threshold]`
/// Scores < 50.0 are red (needs attention)
/// Scores 50.0-79.9 are orange (acceptable)
/// Scores >= 80.0 are green (excellent)
const fn default_overall_scoring_bands() -> [f64; NUM_SCORING_BANDS - 1] {
    [50.0, 80.0]
}

/// Default category-specific scoring thresholds
/// Each category has tailored thresholds based on its importance:
/// - Trustworthiness (security/licenses): highest bar [65.0, 90.0]
/// - Stability (maturity/versions): high bar [60.0, 90.0]
/// - Ownership (maintainers): high bar [55.0, 85.0]
/// - Documentation: moderate bar [50.0, 80.0]
/// - Activity: moderate bar [45.0, 75.0]
/// - Usage: moderate bar [45.0, 75.0]
/// - Community: lower bar [40.0, 70.0]
/// - Cost: lower bar [40.0, 70.0]
fn default_category_scoring_bands() -> HashMap<MetricCategory, [f64; NUM_SCORING_BANDS - 1]> {
    let mut map = HashMap::new();
    let _ = map.insert(MetricCategory::Trustworthiness, [65.0, 90.0]); // Security and legal compliance - critical
    let _ = map.insert(MetricCategory::Stability, [60.0, 90.0]); // Maturity and version stability - very important
    let _ = map.insert(MetricCategory::Ownership, [55.0, 85.0]); // Maintainer count - important for sustainability
    let _ = map.insert(MetricCategory::Documentation, [50.0, 80.0]); // Docs quality - important for usability
    let _ = map.insert(MetricCategory::Activity, [45.0, 75.0]); // Recent activity - moderate (mature crates may be stable)
    let _ = map.insert(MetricCategory::Usage, [45.0, 75.0]); // Downloads/dependents - moderate signal
    let _ = map.insert(MetricCategory::Community, [40.0, 70.0]); // Stars/contributors - nice-to-have
    let _ = map.insert(MetricCategory::Cost, [40.0, 70.0]); // Dependencies/size - nice-to-have
    map
}

/// Default colors for scoring bands: red, orange, green
const fn default_colors_for_scoring_bands() -> [Color; NUM_SCORING_BANDS] {
    [
        Color(Srgb::new(255, 0, 0)),   // Bad: Red
        Color(Srgb::new(255, 165, 0)), // Good: Orange
        Color(Srgb::new(0, 255, 0)),   // Excellent: Green
    ]
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub license: Vec<LicensePolicy>,

    #[serde(default)]
    pub age: Vec<AgePolicy>,

    #[serde(default)]
    pub min_version: Vec<VersionPolicy>,

    #[serde(default)]
    pub release_count: Vec<AgedCountPolicy>,

    #[serde(default)]
    pub overall_download_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub one_month_download_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub overall_owner_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub user_owner_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub team_owner_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub direct_dependency_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub dependent_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub doc_coverage_percentage: Vec<PercentagePolicy>,

    #[serde(default)]
    pub broken_doc_link_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub code_coverage_percentage: Vec<PercentagePolicy>,

    #[serde(default)]
    pub fully_safe_code: Vec<BooleanPolicy>,

    #[serde(default)]
    pub transitive_dependency_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub example_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub repo_star_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub repo_fork_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub repo_subscriber_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub repo_contributor_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub commit_activity: Vec<AgedCountPolicy>,

    #[serde(default)]
    pub open_issue_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub closed_issue_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub issue_responsiveness: Vec<ResponsivenessPolicy>,

    #[serde(default)]
    pub open_pull_request_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub closed_pull_request_count: Vec<MinCountPolicy>,

    #[serde(default)]
    pub pull_request_responsiveness: Vec<ResponsivenessPolicy>,

    #[serde(default)]
    pub vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub low_vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub medium_vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub high_vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub critical_vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub notice_warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub unmaintained_warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub unsound_warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub yanked_warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_low_vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_medium_vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_high_vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_critical_vulnerability_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_notice_warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_unmaintained_warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_unsound_warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub historical_yanked_warning_count: Vec<MaxCountPolicy>,

    #[serde(default)]
    pub metric_scaling: HashMap<Metric, f64>,

    #[serde(default = "default_overall_scoring_bands")]
    pub overall_scoring_bands: [f64; NUM_SCORING_BANDS - 1],

    #[serde(default = "default_category_scoring_bands")]
    pub category_scoring_bands: HashMap<MetricCategory, [f64; NUM_SCORING_BANDS - 1]>,

    #[serde(default = "default_colors_for_scoring_bands")]
    pub colors_for_scoring_bands: [Color; NUM_SCORING_BANDS],

    /// Number of days to keep  crates.io cache data before re-downloading
    #[serde(default = "default_crates_cache_ttl")]
    pub crates_cache_ttl: u64,

    /// Number of days to keep hosting cache data before re-fetching
    #[serde(default = "default_hosting_cache_ttl")]
    pub hosting_cache_ttl: u64,

    /// Number of days to keep cached source codebase before re-fetching
    #[serde(default = "default_source_code_cache_ttl")]
    pub source_code_cache_ttl: u64,

    /// Number of days to keep cached coverage data before re-fetching
    #[serde(default = "default_coverage_cache_ttl")]
    pub coverage_cache_ttl: u64,

    /// Number of days to keep the advisory database cached before re-downloading
    #[serde(default = "default_advisories_cache_ttl")]
    pub advisories_cache_ttl: u64,
}

const fn default_crates_cache_ttl() -> u64 {
    7
}

const fn default_hosting_cache_ttl() -> u64 {
    7
}

const fn default_source_code_cache_ttl() -> u64 {
    7
}

const fn default_coverage_cache_ttl() -> u64 {
    7
}

const fn default_advisories_cache_ttl() -> u64 {
    7
}

impl Config {
    /// Load configuration from a file or use defaults
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed
    pub fn load(workspace_root: &Utf8Path, config_path: Option<&Utf8PathBuf>) -> Result<(Self, Vec<String>)> {
        let (final_path, text) = if let Some(path) = config_path {
            let text = fs::read_to_string(path).into_app_err_with(|| format!("reading cargo-rank configuration from {path}"))?;
            (path.clone(), text)
        } else {
            let candidates = [
                workspace_root.join("rank.toml"),
                workspace_root.join("rank.yml"),
                workspace_root.join("rank.yaml"),
                workspace_root.join("rank.json"),
            ];

            let mut found = None;
            for path in &candidates {
                match fs::read_to_string(path) {
                    Ok(text) => {
                        found = Some((path.clone(), text));
                        break;
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e).into_app_err_with(|| format!("reading cargo-rank configuration from {path}")),
                }
            }

            let Some(result) = found else {
                return Ok((Self::default(), Vec::new()));
            };
            result
        };

        let extension = final_path.extension().unwrap_or_default();
        let config: Self = match extension {
            "toml" => toml::from_str(&text).into_app_err_with(|| format!("parsing TOML configuration from {final_path}"))?,
            "yml" | "yaml" => serde_yaml::from_str(&text).into_app_err_with(|| format!("parsing YAML configuration from {final_path}"))?,
            "json" => serde_json::from_str(&text).into_app_err_with(|| format!("parsing JSON configuration from {final_path}"))?,
            _ => return Err(app_err!("unsupported configuration file extension: {extension}")),
        };

        let mut warnings = Vec::new();
        config.validate(&mut warnings);
        Ok((config, warnings))
    }

    /// Save configuration to a file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written or serialization fails
    pub fn save(&self, output_path: &Utf8Path) -> Result<()> {
        let extension = output_path.extension().unwrap_or_default();
        let text = match extension {
            "toml" => toml::to_string_pretty(self)
                .into_app_err_with(|| format!("serializing configuration to TOML for saving to {output_path}"))?,
            "yml" | "yaml" => serde_yaml::to_string(self)
                .into_app_err_with(|| format!("serializing configuration to YAML for saving to {output_path}"))?,
            "json" => serde_json::to_string_pretty(self)
                .into_app_err_with(|| format!("serializing configuration to JSON for saving to {output_path}"))?,
            _ => return Err(app_err!("unsupported configuration file extension: {extension}")),
        };

        fs::write(output_path, text).into_app_err_with(|| format!("writing configuration to {output_path}"))?;
        Ok(())
    }

    /// Convert YAML configuration to TOML format while preserving comments
    ///
    /// # Errors
    ///
    /// Returns an error if YAML parsing or TOML generation fails
    #[expect(
        clippy::too_many_lines,
        reason = "Complex conversion logic requires detailed handling of different TOML types"
    )]
    fn convert_yaml_to_toml_with_comments(yaml_content: &str) -> Result<String> {
        use toml_edit::{Array, ArrayOfTables, DocumentMut, InlineTable, Item, Table, Value};

        // Helper to format comments for TOML
        fn format_comments(comments: &[String]) -> String {
            comments.iter().fold(String::new(), |mut result, comment| {
                result.push_str(comment);
                result.push('\n');
                result
            })
        }

        // Helper function to convert serde_yaml::Value to toml_edit::Value
        fn yaml_to_toml_simple_value(yaml_val: &serde_yaml::Value) -> Result<Value> {
            match yaml_val {
                serde_yaml::Value::Bool(b) => Ok(Value::Boolean(toml_edit::Formatted::new(*b))),
                serde_yaml::Value::Number(n) => n
                    .as_i64()
                    .map(|i| Value::Integer(toml_edit::Formatted::new(i)))
                    .or_else(|| n.as_f64().map(|f| Value::Float(toml_edit::Formatted::new(f))))
                    .ok_or_else(|| app_err!("unsupported number type")),
                serde_yaml::Value::String(s) => Ok(Value::String(toml_edit::Formatted::new(s.clone()))),
                _ => Err(app_err!("not a simple value type")),
            }
        }

        // Parse the YAML to get the actual data
        let yaml_value = serde_yaml::from_str(yaml_content).into_app_err("parsing YAML content")?;

        // Create a new TOML document
        let mut doc = DocumentMut::new();

        // Extract comment blocks from YAML line-by-line
        let mut comment_blocks: HashMap<String, Vec<String>> = HashMap::new();
        let mut current_comments = Vec::new();

        for line in yaml_content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with('#') {
                // This is a comment line
                current_comments.push(line.to_string());
            } else if trimmed.is_empty() {
                // Keep empty lines in comments
                if !current_comments.is_empty() {
                    current_comments.push(String::new());
                }
            } else if let Some(key_end) = trimmed.find(':') {
                // This looks like a key-value line
                let key = trimmed
                    .get(..key_end)
                    .ok_or_else(|| app_err!("invalid key index"))?
                    .trim()
                    .to_string();
                if !current_comments.is_empty() {
                    let _ = comment_blocks.insert(key.clone(), current_comments.clone());
                    current_comments.clear();
                }
            }
        }

        // Process the YAML mapping
        if let serde_yaml::Value::Mapping(root_map) = yaml_value {
            for (key, value) in root_map {
                if let serde_yaml::Value::String(key_str) = key {
                    // Convert and add the value based on type first
                    match value {
                        // Simple array (numbers, strings, etc.)
                        serde_yaml::Value::Sequence(seq) if !seq.is_empty() && !matches!(seq[0], serde_yaml::Value::Mapping(_)) => {
                            let mut arr = Array::new();
                            for item in seq {
                                if let Ok(val) = yaml_to_toml_simple_value(&item) {
                                    arr.push(val);
                                }
                            }
                            doc[&key_str] = Item::Value(Value::Array(arr));
                        }
                        // Array of tables (objects)
                        serde_yaml::Value::Sequence(seq) if !seq.is_empty() && matches!(seq[0], serde_yaml::Value::Mapping(_)) => {
                            let mut array_of_tables = ArrayOfTables::new();
                            for item in seq {
                                if let serde_yaml::Value::Mapping(map) = item {
                                    let mut table = Table::new();
                                    for (k, v) in map {
                                        if let serde_yaml::Value::String(field_key) = k {
                                            match v {
                                                serde_yaml::Value::Sequence(inner_seq) => {
                                                    let mut arr = Array::new();
                                                    for inner_item in inner_seq {
                                                        if let Ok(val) = yaml_to_toml_simple_value(&inner_item) {
                                                            arr.push(val);
                                                        }
                                                    }
                                                    table[&field_key] = Item::Value(Value::Array(arr));
                                                }
                                                _ => {
                                                    if let Ok(val) = yaml_to_toml_simple_value(&v) {
                                                        table[&field_key] = Item::Value(val);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    array_of_tables.push(table);
                                }
                            }
                            doc[&key_str] = Item::ArrayOfTables(array_of_tables);
                        }
                        // Empty array
                        serde_yaml::Value::Sequence(_) => {
                            doc[&key_str] = Item::Value(Value::Array(Array::new()));
                        }
                        // Mapping (inline table)
                        serde_yaml::Value::Mapping(map) => {
                            let mut table = InlineTable::new();
                            for (k, v) in map {
                                if let serde_yaml::Value::String(field_key) = k {
                                    match v {
                                        serde_yaml::Value::Sequence(seq) => {
                                            // For arrays within mappings, convert to array
                                            let mut arr = Array::new();
                                            for item in seq {
                                                if let Ok(val) = yaml_to_toml_simple_value(&item) {
                                                    arr.push(val);
                                                }
                                            }
                                            let _ = table.insert(&field_key, Value::Array(arr));
                                        }
                                        _ => {
                                            if let Ok(val) = yaml_to_toml_simple_value(&v) {
                                                let _ = table.insert(&field_key, val);
                                            }
                                        }
                                    }
                                }
                            }
                            doc[&key_str] = Item::Value(Value::InlineTable(table));
                        }
                        // Simple values
                        _ => {
                            if let Ok(val) = yaml_to_toml_simple_value(&value) {
                                doc[&key_str] = Item::Value(val);
                            }
                        }
                    }

                    // Now add comments after the item has been inserted
                    // Skip comments for array of tables as they need special handling
                    if let Some(comments) = comment_blocks.get(&key_str) {
                        let comment_text = format_comments(comments);
                        if !comment_text.is_empty() {
                            // Only add comments for non-array-of-tables items
                            if let Some(item) = doc.as_table().get(&key_str)
                                && !matches!(item, Item::ArrayOfTables(_))
                                && let Some(mut key) = doc.as_table_mut().key_mut(&key_str)
                            {
                                key.leaf_decor_mut().set_prefix(comment_text);
                            }
                        }
                    }
                }
            }
        }

        Ok(doc.to_string())
    }

    /// Save the default configuration to a file, preserving comments for YAML format
    ///
    /// When saving to YAML format, this method writes the raw content from `default_config.yml`,
    /// preserving all comments and formatting. For TOML format, it converts the YAML to TOML
    /// while attempting to preserve comments. For other formats (JSON), it serializes
    /// the configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written
    pub fn save_default_with_comments(&self, output_path: &Utf8Path) -> Result<()> {
        let extension = output_path.extension().unwrap_or_default();

        if matches!(extension, "yml" | "yaml") {
            // For YAML, write the raw default content with comments preserved
            fs::write(output_path, DEFAULT_CONFIG_YAML).into_app_err_with(|| format!("writing default configuration to {output_path}"))?;
        } else if extension == "toml" {
            // For TOML, convert from YAML while preserving comments
            let toml_content = Self::convert_yaml_to_toml_with_comments(DEFAULT_CONFIG_YAML)?;
            fs::write(output_path, toml_content).into_app_err_with(|| format!("writing default configuration to {output_path}"))?;
        } else {
            // For other formats, fall back to regular serialization
            self.save(output_path)?;
        }

        Ok(())
    }

    /// Get the color for a given score based on the scoring bands
    ///
    /// Returns:
    /// - Index 0 (bad color) if score is below the first threshold
    /// - Index 1 (good color) if score is between the two thresholds
    /// - Index 2 (excellent color) if score is at or above the second threshold
    /// - None if score is negative (indicates missing/invalid data)
    #[must_use]
    pub fn color_index_for_score(&self, score: f64) -> Option<usize> {
        if score < 0.0 {
            return None;
        }

        if self.overall_scoring_bands.len() >= 2 {
            if score >= self.overall_scoring_bands[1] {
                Some(2) // Excellent
            } else if score >= self.overall_scoring_bands[0] {
                Some(1) // Good
            } else {
                Some(0) // Bad
            }
        } else {
            None
        }
    }

    /// Get the color index for a category score based on category-specific thresholds
    ///
    /// Returns:
    /// - Index 0 (bad color) if score is below the first threshold
    /// - Index 1 (good color) if score is between the two thresholds
    /// - Index 2 (excellent color) if score is at or above the second threshold
    /// - None if score is negative, or if no thresholds are defined for the category
    #[must_use]
    pub fn color_index_for_category_score(&self, category: MetricCategory, score: f64) -> Option<usize> {
        if score < 0.0 {
            return None;
        }

        if let Some(bands) = self.category_scoring_bands.get(&category)
            && bands.len() >= 2
        {
            if score >= bands[1] {
                Some(2) // Excellent
            } else if score >= bands[0] {
                Some(1) // Good
            } else {
                Some(0) // Bad
            }
        } else {
            // Fall back to overall scoring bands if no category-specific bands exist
            self.color_index_for_score(score)
        }
    }

    /// Validate the configuration to detect non-sensical or unreachable policies
    fn validate(&self, warnings: &mut Vec<String>) {
        warnings.extend(LicensePolicy::validate(Metric::License, &self.license));
        warnings.extend(AgePolicy::validate(Metric::Age, &self.age));
        warnings.extend(VersionPolicy::validate(Metric::MinVersion, &self.min_version));
        warnings.extend(MinCountPolicy::validate(Metric::OverallDownloadCount, &self.overall_download_count));
        warnings.extend(MinCountPolicy::validate(
            Metric::OneMonthDownloadCount,
            &self.one_month_download_count,
        ));
        warnings.extend(MinCountPolicy::validate(Metric::OverallOwnerCount, &self.overall_owner_count));
        warnings.extend(MinCountPolicy::validate(Metric::UserOwnerCount, &self.user_owner_count));
        warnings.extend(MinCountPolicy::validate(Metric::TeamOwnerCount, &self.team_owner_count));
        warnings.extend(MaxCountPolicy::validate(
            Metric::DirectDependencyCount,
            &self.direct_dependency_count,
        ));
        warnings.extend(MinCountPolicy::validate(Metric::DependentCount, &self.dependent_count));
        warnings.extend(MaxCountPolicy::validate(Metric::BrokenDocLinkCount, &self.broken_doc_link_count));
        warnings.extend(PercentagePolicy::validate(
            Metric::DocCoveragePercentage,
            &self.doc_coverage_percentage,
        ));
        warnings.extend(PercentagePolicy::validate(
            Metric::CodeCoveragePercentage,
            &self.code_coverage_percentage,
        ));
        warnings.extend(BooleanPolicy::validate(Metric::FullySafeCode, &self.fully_safe_code));
        warnings.extend(MaxCountPolicy::validate(
            Metric::TransitiveDependencyCount,
            &self.transitive_dependency_count,
        ));
        warnings.extend(MinCountPolicy::validate(Metric::ExampleCount, &self.example_count));
        warnings.extend(MinCountPolicy::validate(Metric::RepoStarCount, &self.repo_star_count));
        warnings.extend(MinCountPolicy::validate(Metric::RepoForkCount, &self.repo_fork_count));
        warnings.extend(MinCountPolicy::validate(Metric::RepoSubscriberCount, &self.repo_subscriber_count));
        warnings.extend(MinCountPolicy::validate(Metric::RepoContributorCount, &self.repo_contributor_count));
        warnings.extend(AgedCountPolicy::validate(Metric::ReleaseCount, &self.release_count));
        warnings.extend(AgedCountPolicy::validate(Metric::CommitActivity, &self.commit_activity));
        warnings.extend(MaxCountPolicy::validate(Metric::OpenIssueCount, &self.open_issue_count));
        warnings.extend(MinCountPolicy::validate(Metric::ClosedIssueCount, &self.closed_issue_count));
        warnings.extend(ResponsivenessPolicy::validate(
            Metric::IssueResponsiveness,
            &self.issue_responsiveness,
        ));
        warnings.extend(MaxCountPolicy::validate(
            Metric::OpenPullRequestCount,
            &self.open_pull_request_count,
        ));
        warnings.extend(MinCountPolicy::validate(
            Metric::ClosedPullRequestCount,
            &self.closed_pull_request_count,
        ));
        warnings.extend(ResponsivenessPolicy::validate(
            Metric::PullRequestResponsiveness,
            &self.pull_request_responsiveness,
        ));
    }
}

impl Default for Config {
    fn default() -> Self {
        serde_yaml::from_str(DEFAULT_CONFIG_YAML).expect("default_config.yml should be valid YAML that deserializes to Config")
    }
}
