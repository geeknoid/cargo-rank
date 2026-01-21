//! Generic serialization and deserialization utilities for cache documents.

use crate::Result;
use chrono::{DateTime, Utc};
use core::time::Duration;
use ohno::IntoAppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

const LOG_TARGET: &str = " cache_doc";

/// Load a document from a file
pub fn load<T>(path: impl AsRef<Path>, context: impl AsRef<str>) -> Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let path = path.as_ref();
    let ctx = context.as_ref();
    let should_log = !ctx.is_empty();

    let file = File::open(path).into_app_err_with(|| format!("unable to open file '{}'", path.display()))?;
    let reader = BufReader::new(file);
    let data = serde_json::from_reader(reader).into_app_err_with(|| format!("unable to parse file '{}'", path.display()))?;

    if should_log {
        log::debug!(target: LOG_TARGET, "Cache hit for {ctx}");
    }

    Ok(data)
}

/// Load a document from a file with TTL checking.
pub fn load_with_ttl<T, F>(path: impl AsRef<Path>, ttl: Duration, get_timestamp: F, context: impl AsRef<str>) -> Option<T>
where
    T: for<'de> Deserialize<'de>,
    F: FnOnce(&T) -> DateTime<Utc>,
{
    let path = path.as_ref();
    let ctx = context.as_ref();
    let should_log = !ctx.is_empty();

    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            if should_log {
                log::debug!(target: LOG_TARGET, "Cache miss for {ctx}: {e:#}");
            }
            return None;
        }
    };

    let reader = BufReader::new(file);
    let data = match serde_json::from_reader(reader) {
        Ok(data) => data,
        Err(e) => {
            if should_log {
                log::debug!(target: LOG_TARGET, "Cache miss for {ctx}: {e:#}");
            }
            return None;
        }
    };

    let timestamp = get_timestamp(&data);
    let now = Utc::now();

    // Handle future timestamps (clock skew) - treat as fresh data
    let age = now.signed_duration_since(timestamp);
    if age.num_seconds() < 0 {
        if should_log {
            log::debug!(target: LOG_TARGET, "Cache timestamp is in the future for {ctx} (clock skew detected), treating as fresh");
        }
        return Some(data);
    }

    let age_duration = age.to_std().unwrap_or(Duration::MAX);

    if age_duration < ttl {
        if should_log {
            log::debug!(target: LOG_TARGET, "Cache hit for {ctx} (age: {:.1} days)", age_duration.as_secs_f64() / 86400.0);
        }
        Some(data)
    } else {
        if should_log {
            log::debug!(target: LOG_TARGET,
                "Cache expired for {ctx} (age: {:.1} days, TTL: {:.1} days)",
                age_duration.as_secs_f64() / 86400.0,
                ttl.as_secs_f64() / 86400.0
            );
        }
        None
    }
}

/// Save a document to a file
pub fn save<T>(data: &T, path: impl AsRef<Path>) -> Result<()>
where
    T: Serialize,
{
    let path = path.as_ref();

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).into_app_err_with(|| format!("unable to create directory '{}'", parent.display()))?;
    }

    let file = File::create(path).into_app_err_with(|| format!("unable to create cache file '{}'", path.display()))?;
    let mut writer = BufWriter::new(file);

    // Use pretty formatting in debug mode for easier inspection, compact in release for smaller files
    #[cfg(debug_assertions)]
    let result = serde_json::to_writer_pretty(&mut writer, data);
    #[cfg(not(debug_assertions))]
    let result = serde_json::to_writer(&mut writer, data);

    result.into_app_err_with(|| format!("unable to write cache file '{}'", path.display()))?;
    writer
        .flush()
        .into_app_err_with(|| format!("unable to flush cache file '{}'", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use std::env;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct TestData {
        name: String,
        value: u64,
    }

    #[test]
    fn test_save_and_load_json() {
        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join("cargo_rank_test_json.json");

        let original = TestData {
            name: "test".to_string(),
            value: 42,
        };

        // Save
        save(&original, &file_path).unwrap();

        // Verify file exists
        assert!(file_path.exists());

        // Load (empty context to suppress logging in tests)
        let loaded: TestData = load(&file_path, "").unwrap();

        // Verify data matches
        assert_eq!(original, loaded);

        // Clean up
        let _ = fs::remove_file(&file_path);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let result: Result<TestData> = load("/nonexistent/path/file.json", "");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unable to open"));
    }

    #[test]
    fn test_load_invalid_json() {
        let temp_dir = env::temp_dir();
        let file_path = temp_dir.join("cargo_rank_test_invalid.json");

        // Write invalid JSON
        fs::write(&file_path, "not valid json").unwrap();

        let result: Result<TestData> = load(&file_path, "");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unable to parse"));

        // Clean up
        let _ = fs::remove_file(&file_path);
    }
}
