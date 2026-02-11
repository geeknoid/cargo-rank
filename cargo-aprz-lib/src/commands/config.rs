use crate::Result;
use crate::expr::Expression;
use camino::{Utf8Path, Utf8PathBuf};
use core::time::Duration;
use ohno::{IntoAppError, app_err};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;

/// The default configuration TOML content, embedded from `default_config.toml`
pub const DEFAULT_CONFIG_TOML: &str = include_str!("../../default_config.toml");

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Expressions that if ANY evaluate to true, the crate is flagged as high risk
    #[serde(default)]
    pub high_risk_if_any: Vec<Expression>,

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
    #[serde(default = "default_crates_cache_ttl", with = "humantime_serde")]
    pub crates_cache_ttl: Duration,

    /// Duration to keep hosting cache data before re-fetching
    #[serde(default = "default_hosting_cache_ttl", with = "humantime_serde")]
    pub hosting_cache_ttl: Duration,

    /// Duration to keep cached codebases before re-fetching
    #[serde(default = "default_codebase_cache_ttl", with = "humantime_serde")]
    pub codebase_cache_ttl: Duration,

    /// Duration to keep cached coverage data before re-fetching
    #[serde(default = "default_coverage_cache_ttl", with = "humantime_serde")]
    pub coverage_cache_ttl: Duration,

    /// Duration to keep the advisory database cached before re-downloading
    #[serde(default = "default_advisories_cache_ttl", with = "humantime_serde")]
    pub advisories_cache_ttl: Duration,
}

const fn default_medium_risk_threshold() -> f64 {
    30.0
}

const fn default_low_risk_threshold() -> f64 {
    70.0
}

const fn default_crates_cache_ttl() -> Duration {
    Duration::from_hours(24 * 7)
}

const fn default_hosting_cache_ttl() -> Duration {
    Duration::from_hours(24 * 7)
}

const fn default_codebase_cache_ttl() -> Duration {
    Duration::from_hours(24 * 7)
}

const fn default_coverage_cache_ttl() -> Duration {
    Duration::from_hours(24 * 7)
}

const fn default_advisories_cache_ttl() -> Duration {
    Duration::from_hours(24 * 7)
}

impl Config {
    /// Load configuration from a file or use defaults
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed
    pub fn load(workspace_root: &Utf8Path, config_path: Option<&Utf8PathBuf>) -> Result<Self> {
        let (final_path, text) = if let Some(path) = config_path {
            let text = fs::read_to_string(path).into_app_err_with(|| format!("reading cargo-aprz configuration from {path}"))?;
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
                Err(e) => return Err(e).into_app_err_with(|| format!("reading cargo-aprz configuration from {path}")),
            }
        };

        let config: Self = toml::from_str(&text).into_app_err_with(|| format!("parsing configuration from {final_path}"))?;
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
