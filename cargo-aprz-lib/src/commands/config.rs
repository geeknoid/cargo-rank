use crate::Result;
use crate::expr::Expression;
use camino::{Utf8Path, Utf8PathBuf};
use core::time::Duration;
use ohno::IntoAppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;

/// The default configuration TOML content, embedded from `default_config.toml`
pub const DEFAULT_CONFIG_TOML: &str = include_str!("../../default_config.toml");

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Expressions that if ANY evaluate to true, the crate is denied/rejected
    #[serde(default)]
    pub deny_if_any: Vec<Expression>,

    /// Expressions that if ANY evaluate to true, the crate is accepted (bypassing other checks)
    #[serde(default)]
    pub accept_if_any: Vec<Expression>,

    /// Expressions that must ALL evaluate to true for the crate to be accepted
    #[serde(default)]
    pub accept_if_all: Vec<Expression>,

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
    Duration::from_secs(7 * 24 * 60 * 60) // 7 days
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
}

impl Default for Config {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG_TOML).expect("default_config.toml should be valid TOML that deserializes to Config")
    }
}
