//! A reusable cache backed by JSON files with TTL-aware loading.
//!
//! [`Cache`] wraps a cache directory, TTL, and timestamp so that callers
//! don't need to thread those values through every load/save call.

use crate::Result;
use chrono::{DateTime, Utc};
use core::time::Duration;
use ohno::IntoAppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

const LOG_TARGET: &str = "     cache";

/// Result of loading an entry from the cache.
#[derive(Debug, Clone)]
pub enum CacheResult<T> {
    /// Cached data was found and is still fresh.
    Data(T),

    /// A negative cache entry exists — the data was previously determined to be unavailable.
    NoData(String),

    /// No usable cache entry exists (miss, expired, corrupt, or `ignore_cache` is set).
    Miss,
}

/// On-disk representation of a cache entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
struct Envelope<T> {
    timestamp: DateTime<Utc>,
    payload: EnvelopePayload<T>,
}

/// The payload within an [`Envelope`].
#[derive(Debug, Clone, Deserialize, Serialize)]
enum EnvelopePayload<T> {
    /// Actual cached data.
    Data(T),

    /// Data is not available, with a reason explaining why.
    NoData(String),
}

/// A TTL-aware, directory-backed JSON cache.
#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
    ttl: Duration,
    now: DateTime<Utc>,
    ignore: bool,
}

impl Cache {
    /// Create a new cache.
    #[must_use]
    pub fn new(cache_dir: impl Into<PathBuf>, cache_ttl: Duration, now: DateTime<Utc>, ignore_cache: bool) -> Self {
        Self {
            dir: cache_dir.into(),
            ttl: cache_ttl,
            now,
            ignore: ignore_cache,
        }
    }

    /// Returns the cache directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Returns the timestamp used by this cache.
    #[must_use]
    pub const fn now(&self) -> DateTime<Utc> {
        self.now
    }

    /// Load a cache entry by filename (relative to the cache directory).
    #[must_use]
    pub fn load<T>(&self, filename: &str) -> CacheResult<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        if self.ignore {
            return CacheResult::Miss;
        }

        let path = self.dir.join(filename);

        let file = match File::open(&path) {
            Ok(file) => file,
            Err(e) => {
                log::debug!(target: LOG_TARGET, "Cache miss for {filename}: {e:#}");
                return CacheResult::Miss;
            }
        };

        let reader = BufReader::new(file);
        let envelope: Envelope<T> = match serde_json::from_reader(reader) {
            Ok(data) => data,
            Err(e) => {
                log::debug!(target: LOG_TARGET, "Cache miss for {filename}: {e:#}");
                return CacheResult::Miss;
            }
        };

        // Handle future timestamps (clock skew) — treat as fresh data
        let age = self.now.signed_duration_since(envelope.timestamp);
        if age.num_seconds() < 0 {
            log::debug!(target: LOG_TARGET, "Cache timestamp is in the future for {filename} (clock skew detected), treating as fresh");
        } else {
            let age_duration = age.to_std().unwrap_or(Duration::MAX);

            if age_duration >= self.ttl {
                log::debug!(
                    target: LOG_TARGET,
                    "Cache expired for {filename} (age: {:.1} days, TTL: {:.1} days)",
                    age_duration.as_secs_f64() / 86400.0,
                    self.ttl.as_secs_f64() / 86400.0
                );
                return CacheResult::Miss;
            }

            log::debug!(target: LOG_TARGET, "Cache hit for {filename} (age: {:.1} days)", age_duration.as_secs_f64() / 86400.0);
        }

        match envelope.payload {
            EnvelopePayload::Data(data) => CacheResult::Data(data),
            EnvelopePayload::NoData(reason) => CacheResult::NoData(reason),
        }
    }

    /// Save data to the cache under the given filename.
    pub fn save<T>(&self, filename: &str, data: &T) -> Result<()>
    where
        T: Serialize,
    {
        let envelope = Envelope {
            timestamp: self.now,
            payload: EnvelopePayload::Data(data),
        };
        self.write_envelope(filename, &envelope)
    }

    /// Save a negative cache entry (data unavailable) under the given filename.
    pub fn save_no_data(&self, filename: &str, reason: &str) -> Result<()> {
        // The type parameter doesn't matter for NoData; we use `()` as a placeholder.
        let envelope = Envelope::<()> {
            timestamp: self.now,
            payload: EnvelopePayload::NoData(reason.to_string()),
        };
        self.write_envelope(filename, &envelope)
    }

    /// Write an envelope to disk.
    fn write_envelope<T: Serialize>(&self, filename: &str, envelope: &Envelope<T>) -> Result<()> {
        let path = self.dir.join(filename);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).into_app_err_with(|| format!("creating directory '{}'", parent.display()))?;
        }

        let file = File::create(&path).into_app_err_with(|| format!("creating cache file '{}'", path.display()))?;
        let mut writer = BufWriter::new(file);

        #[cfg(debug_assertions)]
        let result = serde_json::to_writer_pretty(&mut writer, envelope);
        #[cfg(not(debug_assertions))]
        let result = serde_json::to_writer(&mut writer, envelope);

        result.into_app_err_with(|| format!("writing cache file '{}'", path.display()))?;
        writer
            .flush()
            .into_app_err_with(|| format!("flushing cache file '{}'", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize, Clone)]
    struct TestData {
        name: String,
        value: u64,
    }

    fn make_cache(dir: &Path, ttl_secs: u64) -> Cache {
        Cache::new(dir, Duration::from_secs(ttl_secs), Utc::now(), false)
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn save_and_load_data() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = make_cache(tmp.path(), 3600);

        let data = TestData { name: "test".to_string(), value: 42 };
        cache.save("item.json", &data).unwrap();

        match cache.load::<TestData>("item.json") {
            CacheResult::Data(loaded) => assert_eq!(loaded, data),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn save_and_load_no_data() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = make_cache(tmp.path(), 3600);

        cache.save_no_data("missing.json", "not found").unwrap();

        match cache.load::<TestData>("missing.json") {
            CacheResult::NoData(reason) => assert_eq!(reason, "not found"),
            other => panic!("expected NoData, got {other:?}"),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetFullPathNameW")]
    fn load_nonexistent_file() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = make_cache(tmp.path(), 3600);

        assert!(matches!(cache.load::<TestData>("nope.json"), CacheResult::Miss));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn load_invalid_json() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("bad.json"), "not valid json").unwrap();
        let cache = make_cache(tmp.path(), 3600);

        assert!(matches!(cache.load::<TestData>("bad.json"), CacheResult::Miss));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn load_expired_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let old_time = Utc::now() - chrono::Duration::hours(2);

        // Write an envelope with an old timestamp directly
        let envelope = Envelope {
            timestamp: old_time,
            payload: EnvelopePayload::Data(TestData { name: "old".to_string(), value: 1 }),
        };
        let path = tmp.path().join("old.json");
        let file = File::create(&path).unwrap();
        serde_json::to_writer(file, &envelope).unwrap();

        let cache = make_cache(tmp.path(), 3600);
        assert!(matches!(cache.load::<TestData>("old.json"), CacheResult::Miss));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn load_future_timestamp_treated_as_fresh() {
        let tmp = tempfile::tempdir().unwrap();
        let future_time = Utc::now() + chrono::Duration::hours(1);

        let envelope = Envelope {
            timestamp: future_time,
            payload: EnvelopePayload::Data(TestData { name: "future".to_string(), value: 1 }),
        };
        let path = tmp.path().join("future.json");
        let file = File::create(&path).unwrap();
        serde_json::to_writer(file, &envelope).unwrap();

        let cache = make_cache(tmp.path(), 3600);
        match cache.load::<TestData>("future.json") {
            CacheResult::Data(d) => assert_eq!(d.name, "future"),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn ignore_cache_returns_miss() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = Cache::new(tmp.path(), Duration::from_secs(3600), Utc::now(), true);

        let data = TestData { name: "ignored".to_string(), value: 1 };
        // Save via a non-ignoring cache so the file actually exists
        make_cache(tmp.path(), 3600).save("item.json", &data).unwrap();

        assert!(matches!(cache.load::<TestData>("item.json"), CacheResult::Miss));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn save_creates_parent_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = make_cache(tmp.path(), 3600);

        let data = TestData { name: "nested".to_string(), value: 123 };
        cache.save("sub/dir/item.json", &data).unwrap();

        match cache.load::<TestData>("sub/dir/item.json") {
            CacheResult::Data(loaded) => assert_eq!(loaded, data),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn save_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = make_cache(tmp.path(), 3600);

        cache.save("item.json", &TestData { name: "first".to_string(), value: 1 }).unwrap();
        cache.save("item.json", &TestData { name: "second".to_string(), value: 2 }).unwrap();

        match cache.load::<TestData>("item.json") {
            CacheResult::Data(loaded) => assert_eq!(loaded.name, "second"),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    fn dir_accessor_returns_cache_dir() {
        let cache = Cache::new("/some/path", Duration::from_secs(3600), Utc::now(), false);
        assert_eq!(cache.dir(), Path::new("/some/path"));
    }

    #[test]
    fn now_accessor_returns_timestamp() {
        let now = Utc::now();
        let cache = Cache::new("/tmp", Duration::from_secs(3600), now, false);
        assert_eq!(cache.now(), now);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn save_no_data_then_overwrite_with_data() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = make_cache(tmp.path(), 3600);

        cache.save_no_data("item.json", "originally missing").unwrap();
        assert!(matches!(cache.load::<TestData>("item.json"), CacheResult::NoData(r) if r == "originally missing"));

        let data = TestData { name: "now available".to_string(), value: 99 };
        cache.save("item.json", &data).unwrap();
        match cache.load::<TestData>("item.json") {
            CacheResult::Data(loaded) => assert_eq!(loaded, data),
            other => panic!("expected Data, got {other:?}"),
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn exactly_at_ttl_boundary_is_miss() {
        let tmp = tempfile::tempdir().unwrap();
        let ttl_seconds = 3600i64;
        let old_time = Utc::now() - chrono::Duration::seconds(ttl_seconds);

        let envelope = Envelope {
            timestamp: old_time,
            payload: EnvelopePayload::Data(TestData { name: "boundary".to_string(), value: 1 }),
        };
        let path = tmp.path().join("boundary.json");
        let file = File::create(&path).unwrap();
        serde_json::to_writer(file, &envelope).unwrap();

        let cache = make_cache(tmp.path(), ttl_seconds.cast_unsigned());
        assert!(matches!(cache.load::<TestData>("boundary.json"), CacheResult::Miss));
    }
}
