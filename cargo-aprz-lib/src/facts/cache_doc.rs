//! Cache envelope for timestamped, TTL-aware caching of serializable data.

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

/// The payload within a [`CacheEnvelope`].
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum EnvelopePayload<T> {
    /// Actual cached data.
    Data(T),

    /// Data is not available, with a reason explaining why.
    NoData(String),
}

impl<T> EnvelopePayload<T> {
    /// Returns the data if this is a `Data` variant, or `None`.
    #[cfg(any(test, debug_assertions))]
    #[must_use]
    pub fn into_data(self) -> Option<T> {
        match self {
            Self::Data(data) => Some(data),
            Self::NoData(_) => None,
        }
    }

    /// Returns `true` if this is a `NoData` variant.
    #[cfg(any(test, debug_assertions))]
    #[must_use]
    pub const fn is_no_data(&self) -> bool {
        matches!(self, Self::NoData(_))
    }
}

/// A cache wrapper that stores a timestamp alongside a payload.
///
/// The payload can be either actual data or a reason explaining why data is unavailable.
/// The TTL applies equally to both cases.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CacheEnvelope<T> {
    pub timestamp: DateTime<Utc>,
    pub payload: EnvelopePayload<T>,
}

impl<T> CacheEnvelope<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    /// Create an envelope with a data payload.
    #[must_use]
    pub const fn data(timestamp: DateTime<Utc>, payload: T) -> Self {
        Self {
            timestamp,
            payload: EnvelopePayload::Data(payload),
        }
    }

    /// Create an envelope representing unavailable data.
    #[must_use]
    pub fn no_data(timestamp: DateTime<Utc>, reason: impl Into<String>) -> Self {
        Self {
            timestamp,
            payload: EnvelopePayload::NoData(reason.into()),
        }
    }

    /// Load an envelope from a cache file, returning `None` on miss or expiry.
    pub fn load(
        path: impl AsRef<Path>,
        ttl: Duration,
        now: DateTime<Utc>,
        context: impl AsRef<str>,
    ) -> Option<Self> {
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
        let envelope: Self = match serde_json::from_reader(reader) {
            Ok(data) => data,
            Err(e) => {
                log::debug!(target: LOG_TARGET, "Cache miss for {ctx}: {e:#}");
                return None;
            }
        };

        // Handle future timestamps (clock skew) - treat as fresh data
        let age = now.signed_duration_since(envelope.timestamp);
        if age.num_seconds() < 0 {
            log::debug!(target: LOG_TARGET, "Cache timestamp is in the future for {ctx} (clock skew detected), treating as fresh");
            return Some(envelope);
        }

        let age_duration = age.to_std().unwrap_or(Duration::MAX);

        if age_duration < ttl {
            log::debug!(target: LOG_TARGET, "Cache hit for {ctx} (age: {:.1} days)", age_duration.as_secs_f64() / 86400.0);
            Some(envelope)
        } else {
            log::debug!(target: LOG_TARGET,
                "Cache expired for {ctx} (age: {:.1} days, TTL: {:.1} days)",
                age_duration.as_secs_f64() / 86400.0,
                ttl.as_secs_f64() / 86400.0
            );
            None
        }
    }

    /// Save this envelope to a cache file.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).into_app_err_with(|| format!("creating directory '{}'", parent.display()))?;
        }

        let file = File::create(path).into_app_err_with(|| format!("creating cache file '{}'", path.display()))?;
        let mut writer = BufWriter::new(file);

        // Use pretty formatting in debug mode for easier inspection, compact in release for smaller files
        #[cfg(debug_assertions)]
        let result = serde_json::to_writer_pretty(&mut writer, self);
        #[cfg(not(debug_assertions))]
        let result = serde_json::to_writer(&mut writer, self);

        result.into_app_err_with(|| format!("writing cache file '{}'", path.display()))?;
        writer
            .flush()
            .into_app_err_with(|| format!("flushing cache file '{}'", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
    struct TestData {
        name: String,
        value: u64,
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_save_and_load() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("data.json");

        let original = TestData {
            name: "test".to_string(),
            value: 42,
        };

        let envelope = CacheEnvelope::data(Utc::now(), original.clone());
        envelope.save(&file_path).unwrap();

        assert!(file_path.exists());

        let ttl = Duration::from_secs(3600);
        let loaded = CacheEnvelope::<TestData>::load(&file_path, ttl, Utc::now(), "test data");

        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().payload.into_data().unwrap(), original);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_save_and_load_not_found() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("notfound.json");

        let envelope = CacheEnvelope::<TestData>::no_data(Utc::now(), "not found");
        envelope.save(&file_path).unwrap();

        let ttl = Duration::from_secs(3600);
        let loaded = CacheEnvelope::<TestData>::load(&file_path, ttl, Utc::now(), "test data");

        assert!(loaded.is_some());
        assert!(loaded.unwrap().payload.is_no_data());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetFullPathNameW")]
    fn test_load_nonexistent_file() {
        let ttl = Duration::from_secs(3600);
        let loaded = CacheEnvelope::<TestData>::load("/nonexistent/path/file.json", ttl, Utc::now(), "test data");
        assert!(loaded.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_invalid_json() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("invalid.json");
        fs::write(&file_path, "not valid json").unwrap();

        let ttl = Duration::from_secs(3600);
        let loaded = CacheEnvelope::<TestData>::load(&file_path, ttl, Utc::now(), "test data");
        assert!(loaded.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_fresh_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("fresh.json");

        let envelope = CacheEnvelope::data(Utc::now(), TestData { name: "test".to_string(), value: 1 });
        envelope.save(&file_path).unwrap();

        let ttl = Duration::from_secs(3600);
        let loaded = CacheEnvelope::<TestData>::load(&file_path, ttl, Utc::now(), "test data");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().payload.into_data().unwrap().name, "test");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_expired_cache() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("expired.json");

        let envelope = CacheEnvelope::data(
            Utc::now() - chrono::Duration::hours(2),
            TestData { name: "test".to_string(), value: 1 },
        );
        envelope.save(&file_path).unwrap();

        let ttl = Duration::from_secs(3600);
        let loaded = CacheEnvelope::<TestData>::load(&file_path, ttl, Utc::now(), "test data");
        assert!(loaded.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_future_timestamp() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("future.json");

        let envelope = CacheEnvelope::data(
            Utc::now() + chrono::Duration::hours(1),
            TestData { name: "test".to_string(), value: 1 },
        );
        envelope.save(&file_path).unwrap();

        let ttl = Duration::from_secs(3600);
        let loaded = CacheEnvelope::<TestData>::load(&file_path, ttl, Utc::now(), "test data");
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().payload.into_data().unwrap().name, "test");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_load_exactly_at_ttl_boundary() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("boundary.json");

        let ttl_seconds = 3600;
        let envelope = CacheEnvelope::data(
            Utc::now() - chrono::Duration::seconds(ttl_seconds),
            TestData { name: "test".to_string(), value: 1 },
        );
        envelope.save(&file_path).unwrap();

        let ttl = Duration::from_secs(ttl_seconds.cast_unsigned());
        let loaded = CacheEnvelope::<TestData>::load(&file_path, ttl, Utc::now(), "test data");
        assert!(loaded.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_save_creates_parent_directories() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nested_path = temp_dir.path().join("nested").join("subdir").join("data.json");

        let envelope = CacheEnvelope::data(Utc::now(), TestData { name: "nested".to_string(), value: 123 });
        envelope.save(&nested_path).unwrap();
        assert!(nested_path.exists());

        let ttl = Duration::from_secs(3600);
        let loaded = CacheEnvelope::<TestData>::load(&nested_path, ttl, Utc::now(), "nested test");
        assert_eq!(loaded.unwrap().payload.into_data().unwrap().name, "nested");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_save_overwrites_existing_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("overwrite.json");

        let e1 = CacheEnvelope::data(Utc::now(), TestData { name: "first".to_string(), value: 1 });
        e1.save(&file_path).unwrap();

        let e2 = CacheEnvelope::data(Utc::now(), TestData { name: "second".to_string(), value: 2 });
        e2.save(&file_path).unwrap();

        let ttl = Duration::from_secs(3600);
        let loaded = CacheEnvelope::<TestData>::load(&file_path, ttl, Utc::now(), "overwrite test");
        assert_eq!(loaded.unwrap().payload.into_data().unwrap().name, "second");
    }
}
