use crate::Result;
use crate::expr::Expression;
use camino::{Utf8Path, Utf8PathBuf};
use core::time::Duration;
use ohno::{IntoAppError, app_err};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;

/// Serde module that (de)serializes `std::time::Duration` via jiff's friendly duration format.
///
/// Deserialization parses human-readable strings like "1 week" or "2h 30m" through
/// `jiff::Span`, then converts to `Duration` using nominal unit values
/// (1 year = 365 days, 1 month = 30 days).
///
/// Serialization formats the duration using jiff's friendly compact notation.
mod friendly_duration {
    use core::time::Duration;
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let sdur = jiff::SignedDuration::try_from(*duration).map_err(serde::ser::Error::custom)?;
        serializer.collect_str(&format_args!("{sdur:#}"))
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let span: jiff::Span = s.parse().map_err(serde::de::Error::custom)?;
        span_to_duration(span).map_err(serde::de::Error::custom)
    }

    fn span_to_duration(span: jiff::Span) -> Result<Duration, String> {
        let total_nanos: i128 = i128::from(span.get_years()) * 365 * 86_400 * 1_000_000_000
            + i128::from(span.get_months()) * 30 * 86_400 * 1_000_000_000
            + i128::from(span.get_weeks()) * 7 * 86_400 * 1_000_000_000
            + i128::from(span.get_days()) * 86_400 * 1_000_000_000
            + i128::from(span.get_hours()) * 3_600 * 1_000_000_000
            + i128::from(span.get_minutes()) * 60 * 1_000_000_000
            + i128::from(span.get_seconds()) * 1_000_000_000
            + i128::from(span.get_milliseconds()) * 1_000_000
            + i128::from(span.get_microseconds()) * 1_000
            + i128::from(span.get_nanoseconds());

        if total_nanos < 0 {
            return Err("duration must not be negative".into());
        }

        let secs = u64::try_from(total_nanos / 1_000_000_000)
            .map_err(|_err| "duration overflow".to_string())?;
        let nanos = u32::try_from(total_nanos % 1_000_000_000)
            .map_err(|_err| "duration overflow".to_string())?;

        Ok(Duration::new(secs, nanos))
    }
}

/// The default configuration TOML content, embedded from `default_config.toml`
pub const DEFAULT_CONFIG_TOML: &str = include_str!("../../default_config.toml");

/// An entry in the allow list that exempts a specific crate+version from triggering
/// error exit codes when using `--error-if-medium-risk` or `--error-if-high-risk`.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AllowListEntry {
    /// The crate name to allow
    pub name: String,

    /// A semver version requirement (e.g. "^1.0", ">=2.0, <3.0", "=1.2.3", "*")
    pub version: VersionReq,
}

impl AllowListEntry {
    /// Check if this entry matches the given crate name and version.
    #[must_use]
    pub fn matches(&self, name: &str, version: &Version) -> bool {
        self.name == name && self.version.matches(version)
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Crates that are exempt from triggering error exit codes with `--error-if-medium-risk` or `--error-if-high-risk`
    #[serde(default)]
    pub allow_list: Vec<AllowListEntry>,

    /// Expressions that must ALL evaluate to true for the crate to avoid being flagged as high risk
    #[serde(default)]
    pub high_risk: Vec<Expression>,

    /// Expressions that must ALL evaluate to true for the crate to be accepted
    #[serde(default)]
    pub eval: Vec<Expression>,

    /// Score threshold below which a crate is considered medium risk (0..100)
    #[serde(default = "default_medium_risk_threshold")]
    pub medium_risk_threshold: f64,

    /// Score threshold at or above which a crate is considered low risk (0..100)
    #[serde(default = "default_low_risk_threshold")]
    pub low_risk_threshold: f64,

    /// Duration to keep crates.io cache data before re-downloading
    #[serde(default = "default_cache_ttl", with = "friendly_duration")]
    pub crates_cache_ttl: Duration,

    /// Duration to keep hosting cache data before re-fetching
    #[serde(default = "default_cache_ttl", with = "friendly_duration")]
    pub hosting_cache_ttl: Duration,

    /// Duration to keep cached codebases before re-fetching
    #[serde(default = "default_cache_ttl", with = "friendly_duration")]
    pub codebase_cache_ttl: Duration,

    /// Duration to keep cached coverage data before re-fetching
    #[serde(default = "default_cache_ttl", with = "friendly_duration")]
    pub coverage_cache_ttl: Duration,

    /// Duration to keep the advisory database cached before re-downloading
    #[serde(default = "default_cache_ttl", with = "friendly_duration")]
    pub advisories_cache_ttl: Duration,
}

const fn default_medium_risk_threshold() -> f64 {
    30.0
}

const fn default_low_risk_threshold() -> f64 {
    70.0
}

const fn default_cache_ttl() -> Duration {
    Duration::from_hours(24 * 7)
}

impl Config {
    /// Check if a crate is on the allow list.
    #[must_use]
    pub fn is_allowed(&self, name: &str, version: &Version) -> bool {
        self.allow_list.iter().any(|entry| entry.matches(name, version))
    }

    /// Load configuration from a file or use defaults
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed
    pub fn load(workspace_root: &Utf8Path, config_path: Option<&Utf8PathBuf>) -> Result<Self> {
        let (final_path, text) = if let Some(path) = config_path {
            let text = fs::read_to_string(path).into_app_err_with(|| format!("reading cargo-aprz configuration file '{path}'"))?;
            (path.clone(), text)
        } else {
            // Look for aprz.toml
            let path = workspace_root.join("aprz.toml");
            match fs::read_to_string(&path) {
                Ok(text) => (path, text),
                Err(e) if e.kind() == io::ErrorKind::NotFound => {
                    // No config file found, use defaults
                    return Ok(Self::default());
                }
                Err(e) => return Err(e).into_app_err_with(|| format!("reading cargo-aprz configuration file '{path}'")),
            }
        };

        let config: Self = toml::from_str(&text).into_app_err_with(|| format!("parsing configuration file '{final_path}'"))?;
        config.validate()?;

        Ok(config)
    }

    /// Save the default configuration to a TOML file
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written
    pub fn save_default(output_path: &Utf8Path) -> Result<()> {
        fs::write(output_path, DEFAULT_CONFIG_TOML).into_app_err_with(|| format!("writing default configuration to {output_path}"))?;
        Ok(())
    }

    /// Validate configuration values
    ///
    /// # Errors
    ///
    /// Returns an error if threshold values are out of range or inconsistent
    fn validate(&self) -> Result<()> {
        if !(0.0..=100.0).contains(&self.medium_risk_threshold) {
            return Err(app_err!(
                "medium_risk_threshold must be between 0 and 100, got {}",
                self.medium_risk_threshold
            ));
        }

        if !(0.0..=100.0).contains(&self.low_risk_threshold) {
            return Err(app_err!(
                "low_risk_threshold must be between 0 and 100, got {}",
                self.low_risk_threshold
            ));
        }

        if self.medium_risk_threshold >= self.low_risk_threshold {
            return Err(app_err!(
                "medium_risk_threshold ({}) must be less than low_risk_threshold ({})",
                self.medium_risk_threshold,
                self.low_risk_threshold
            ));
        }

        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG_TOML).expect("default_config.toml should be valid TOML that deserializes to Config")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_is_valid() {
        let config = Config::default();
        config.validate().unwrap();
    }

    #[test]
    fn test_validate_medium_risk_out_of_range_low() {
        let config = Config { medium_risk_threshold: -1.0, ..Config::default() };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_medium_risk_out_of_range_high() {
        let config = Config { medium_risk_threshold: 101.0, ..Config::default() };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_low_risk_out_of_range_low() {
        let config = Config { low_risk_threshold: -1.0, ..Config::default() };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_low_risk_out_of_range_high() {
        let config = Config { low_risk_threshold: 101.0, ..Config::default() };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_medium_ge_low_risk() {
        let config = Config { medium_risk_threshold: 80.0, low_risk_threshold: 70.0, ..Config::default() };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_medium_equals_low_risk() {
        let config = Config { medium_risk_threshold: 70.0, low_risk_threshold: 70.0, ..Config::default() };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_valid_thresholds() {
        let config = Config { medium_risk_threshold: 30.0, low_risk_threshold: 70.0, ..Config::default() };
        config.validate().unwrap();
    }

    #[test]
    fn test_validate_boundary_values() {
        let config = Config { medium_risk_threshold: 0.0, low_risk_threshold: 100.0, ..Config::default() };
        config.validate().unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_save_default_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let output_path = Utf8PathBuf::try_from(tmp.path().join("aprz.toml")).unwrap();
        Config::save_default(&output_path).unwrap();
        let loaded = Config::load(&Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap(), Some(&output_path)).unwrap();
        loaded.validate().unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_missing_config_uses_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace_root = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let config = Config::load(&workspace_root, None).unwrap();
        config.validate().unwrap();
    }

    #[test]
    fn test_default_config_toml_is_not_empty() {
        assert!(!DEFAULT_CONFIG_TOML.is_empty());
    }

    #[test]
    fn test_default_config_has_empty_allow_list() {
        let config = Config::default();
        assert!(config.allow_list.is_empty());
    }

    #[test]
    fn test_allow_list_entry_matches_exact_version() {
        let entry = AllowListEntry {
            name: "foo".to_string(),
            version: VersionReq::parse("=1.2.3").unwrap(),
        };
        assert!(entry.matches("foo", &Version::new(1, 2, 3)));
        assert!(!entry.matches("foo", &Version::new(1, 2, 4)));
        assert!(!entry.matches("bar", &Version::new(1, 2, 3)));
    }

    #[test]
    fn test_allow_list_entry_matches_caret_range() {
        let entry = AllowListEntry {
            name: "foo".to_string(),
            version: VersionReq::parse("^1.0").unwrap(),
        };
        assert!(entry.matches("foo", &Version::new(1, 0, 0)));
        assert!(entry.matches("foo", &Version::new(1, 9, 9)));
        assert!(!entry.matches("foo", &Version::new(2, 0, 0)));
    }

    #[test]
    fn test_allow_list_entry_matches_wildcard() {
        let entry = AllowListEntry {
            name: "foo".to_string(),
            version: VersionReq::parse("*").unwrap(),
        };
        assert!(entry.matches("foo", &Version::new(0, 0, 1)));
        assert!(entry.matches("foo", &Version::new(99, 99, 99)));
        assert!(!entry.matches("bar", &Version::new(1, 0, 0)));
    }

    #[test]
    fn test_is_allowed_matches() {
        let mut config = Config::default();
        config.allow_list.push(AllowListEntry {
            name: "foo".to_string(),
            version: VersionReq::parse("^1.0").unwrap(),
        });
        assert!(config.is_allowed("foo", &Version::new(1, 2, 3)));
        assert!(!config.is_allowed("foo", &Version::new(2, 0, 0)));
        assert!(!config.is_allowed("bar", &Version::new(1, 0, 0)));
    }

    #[test]
    fn test_is_allowed_empty_list() {
        let config = Config::default();
        assert!(!config.is_allowed("foo", &Version::new(1, 0, 0)));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_config_with_allow_list() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = Utf8PathBuf::try_from(tmp.path().join("aprz.toml")).unwrap();
        let toml_content = r#"
medium_risk_threshold = 30.0
low_risk_threshold = 70.0

[[allow_list]]
name = "some-crate"
version = "=1.2.3"

[[allow_list]]
name = "another-crate"
version = "^2.0"
"#;
        fs::write(&config_path, toml_content).unwrap();
        let workspace_root = Utf8PathBuf::try_from(tmp.path().to_path_buf()).unwrap();
        let config = Config::load(&workspace_root, Some(&config_path)).unwrap();
        assert_eq!(config.allow_list.len(), 2);
        assert!(config.is_allowed("some-crate", &Version::new(1, 2, 3)));
        assert!(!config.is_allowed("some-crate", &Version::new(1, 2, 4)));
        assert!(config.is_allowed("another-crate", &Version::new(2, 5, 0)));
        assert!(!config.is_allowed("another-crate", &Version::new(1, 0, 0)));
    }
}
