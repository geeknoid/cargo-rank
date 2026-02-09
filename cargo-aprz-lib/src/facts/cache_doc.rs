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

    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            log::debug!(target: LOG_TARGET, "Cache miss for {ctx}: {e:#}");
            return Err(e).into_app_err_with(|| format!("unable to open file '{}'", path.display()));
        }
    };

    let reader = BufReader::new(file);
    let data = match serde_json::from_reader(reader) {
        Ok(data) => data,
        Err(e) => {
            log::debug!(target: LOG_TARGET, "Cache miss for {ctx}: {e:#}");
            return Err(e).into_app_err_with(|| format!("unable to parse file '{}'", path.display()));
        }
    };

    log::debug!(target: LOG_TARGET, "Cache hit for {ctx}");

    Ok(data)
}

/// Load a document from a file with TTL checking.
pub fn load_with_ttl<T, F>(
    path: impl AsRef<Path>,
    ttl: Duration,
    get_timestamp: F,
    now: DateTime<Utc>,
    context: impl AsRef<str>,
) -> Option<T>
where
    T: for<'de> Deserialize<'de>,
    F: FnOnce(&T) -> DateTime<Utc>,
{
    let path = path.as_ref();
    let ctx = context.as_ref();

    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            log::debug!(target: LOG_TARGET, "Cache miss for {ctx}: {e:#}");
            return None;
        }
    };

    let reader = BufReader::new(file);
    let data = match serde_json::from_reader(reader) {
        Ok(data) => data,
        Err(e) => {
            log::debug!(target: LOG_TARGET, "Cache miss for {ctx}: {e:#}");
            return None;
        }
    };

    let timestamp = get_timestamp(&data);

    // Handle future timestamps (clock skew) - treat as fresh data
    let age = now.signed_duration_since(timestamp);
    if age.num_seconds() < 0 {
        log::debug!(target: LOG_TARGET, "Cache timestamp is in the future for {ctx} (clock skew detected), treating as fresh");
        return Some(data);
    }

    let age_duration = age.to_std().unwrap_or(Duration::MAX);

    if age_duration < ttl {
        log::debug!(target: LOG_TARGET, "Cache hit for {ctx} (age: {:.1} days)", age_duration.as_secs_f64() / 86400.0);
        Some(data)
    } else {
        log::debug!(target: LOG_TARGET,
            "Cache expired for {ctx} (age: {:.1} days, TTL: {:.1} days)",
            age_duration.as_secs_f64() / 86400.0,
            ttl.as_secs_f64() / 86400.0
        );
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

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct TestData {
        name: String,
        value: u64,
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_save_and_load_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("data.json");

        let original = TestData {
            name: "test".to_string(),
            value: 42,
        };

        // Save
        save(&original, &file_path).unwrap();

        // Verify file exists
        assert!(file_path.exists());

        // Load
        let loaded: TestData = load(&file_path, "test data").unwrap();

        // Verify data matches
        assert_eq!(original, loaded);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetFullPathNameW")]
    fn test_load_nonexistent_file() {
        let result: Result<TestData> = load("/nonexistent/path/file.json", "test data");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unable to open"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_invalid_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("invalid.json");

        // Write invalid JSON
        fs::write(&file_path, "not valid json").unwrap();

        let result: Result<TestData> = load(&file_path, "test data");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unable to parse"));
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct TimestampedData {
        name: String,
        timestamp: DateTime<Utc>,
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_with_ttl_fresh_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("fresh.json");

        let data = TimestampedData {
            name: "test".to_string(),
            timestamp: Utc::now(),
        };

        save(&data, &file_path).unwrap();

        let ttl = Duration::from_secs(3600); // 1 hour TTL
        let loaded = load_with_ttl(&file_path, ttl, |d: &TimestampedData| d.timestamp, Utc::now(), "test data");

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.name, "test");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_with_ttl_expired_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("expired.json");

        // Create data with old timestamp (2 hours ago)
        let data = TimestampedData {
            name: "test".to_string(),
            timestamp: Utc::now() - chrono::Duration::hours(2),
        };

        save(&data, &file_path).unwrap();

        let ttl = Duration::from_secs(3600); // 1 hour TTL
        let loaded = load_with_ttl(&file_path, ttl, |d: &TimestampedData| d.timestamp, Utc::now(), "test data");

        // Should be None because cache is expired
        assert!(loaded.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_with_ttl_future_timestamp() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("future.json");

        // Create data with future timestamp (clock skew simulation)
        let data = TimestampedData {
            name: "test".to_string(),
            timestamp: Utc::now() + chrono::Duration::hours(1),
        };

        save(&data, &file_path).unwrap();

        let ttl = Duration::from_secs(3600);
        let loaded = load_with_ttl(&file_path, ttl, |d: &TimestampedData| d.timestamp, Utc::now(), "test data");

        // Should be Some - future timestamps are treated as fresh
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.name, "test");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_with_ttl_nonexistent_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("nonexistent.json");

        let ttl = Duration::from_secs(3600);
        let loaded = load_with_ttl::<TimestampedData, _>(&file_path, ttl, |d| d.timestamp, Utc::now(), "test data");

        assert!(loaded.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_with_ttl_invalid_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("invalid.json");

        fs::write(&file_path, "not valid json").unwrap();

        let ttl = Duration::from_secs(3600);
        let loaded = load_with_ttl::<TimestampedData, _>(&file_path, ttl, |d| d.timestamp, Utc::now(), "test data");

        assert!(loaded.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_with_ttl_exactly_at_ttl_boundary() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("boundary.json");

        // Create data with timestamp exactly at TTL boundary
        let ttl_seconds = 3600;
        let data = TimestampedData {
            name: "test".to_string(),
            timestamp: Utc::now() - chrono::Duration::seconds(ttl_seconds),
        };

        save(&data, &file_path).unwrap();

        let ttl = Duration::from_secs(ttl_seconds.cast_unsigned());
        let loaded = load_with_ttl(&file_path, ttl, |d: &TimestampedData| d.timestamp, Utc::now(), "test data");

        // At boundary, should be expired (age >= ttl)
        assert!(loaded.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_save_creates_parent_directories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested_path = temp_dir.path().join("nested").join("subdir").join("data.json");

        let data = TestData {
            name: "nested".to_string(),
            value: 123,
        };

        save(&data, &nested_path).unwrap();

        assert!(nested_path.exists());

        let loaded: TestData = load(&nested_path, "nested test").unwrap();
        assert_eq!(loaded.name, "nested");
        assert_eq!(loaded.value, 123);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_save_overwrites_existing_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("overwrite.json");

        let data1 = TestData {
            name: "first".to_string(),
            value: 1,
        };

        save(&data1, &file_path).unwrap();

        let data2 = TestData {
            name: "second".to_string(),
            value: 2,
        };

        save(&data2, &file_path).unwrap();

        let loaded: TestData = load(&file_path, "overwrite test").unwrap();
        assert_eq!(loaded.name, "second");
        assert_eq!(loaded.value, 2);
    }
}
