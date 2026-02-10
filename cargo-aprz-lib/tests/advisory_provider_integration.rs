//! Integration tests for the advisory provider using the real `RustSec` database.
//!
//! These tests download the `RustSec` advisory database from GitHub, so they
//! require network access. The database is downloaded once and shared across
//! all tests via a static [`tokio::sync::OnceCell`].

#![cfg(feature = "network_tests")]

use cargo_aprz_lib::facts::advisories::{AdvisoryData, Provider};
use cargo_aprz_lib::facts::{CrateSpec, Progress, ProviderResult};
use chrono::Utc;
use semver::Version;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::OnceCell;

/// No-op progress reporter for testing
#[derive(Debug)]
struct NoOpProgress;

impl Progress for NoOpProgress {
    fn set_phase(&self, _phase: &str) {}
    fn set_determinate(&self, _callback: Box<dyn Fn() -> (u64, u64, String) + Send + Sync + 'static>) {}
    fn set_indeterminate(&self, _callback: Box<dyn Fn() -> String + Send + Sync + 'static>) {}
    fn done(&self) {}
}

/// Shared provider state that persists for the lifetime of the test binary.
/// The database is downloaded once on first access and reused by every test.
struct SharedProvider {
    provider: Provider,
    // Keep the temp dir alive so the cached DB isn't deleted.
    _cache_dir: PathBuf,
}

static PROVIDER: OnceCell<SharedProvider> = OnceCell::const_new();

async fn shared_provider() -> &'static Provider {
    &PROVIDER
        .get_or_init(|| async {
            let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
            let cache_path = temp_dir.path().to_path_buf();

            // Leak the TempDir so the directory (and its cached DB) lives
            // for the entire process. The OS cleans it up on exit.
            core::mem::forget(temp_dir);

            let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
            let provider = Provider::new(&cache_path, core::time::Duration::from_secs(365 * 24 * 3600), progress, Utc::now())
                .await
                .expect("Failed to create advisory provider");

            SharedProvider {
                provider,
                _cache_dir: cache_path,
            }
        })
        .await
        .provider
}

/// Helper to create a [`CrateSpec`] from name and version strings.
fn make_spec(name: &str, version: &str) -> CrateSpec {
    CrateSpec::from_arcs(Arc::from(name), Arc::new(Version::parse(version).expect("valid version")))
}

#[tokio::test]
async fn test_advisory_provider_known_vulnerable_crate() {
    let provider = shared_provider().await;

    // `hyper` 0.14.10 has known advisories (e.g. RUSTSEC-2021-0078, RUSTSEC-2021-0079)
    let crate_spec = make_spec("hyper", "0.14.10");
    let results: Vec<(CrateSpec, ProviderResult<AdvisoryData>)> = provider.get_advisory_data(vec![crate_spec]).await.collect();

    assert_eq!(results.len(), 1);
    let (spec, result) = &results[0];
    assert_eq!(spec.name(), "hyper");

    match result {
        ProviderResult::Found(data) => {
            // hyper has had multiple advisories across versions
            let total_advisories = data.total.low_vulnerability_count
                + data.total.medium_vulnerability_count
                + data.total.high_vulnerability_count
                + data.total.critical_vulnerability_count
                + data.total.notice_warning_count
                + data.total.unmaintained_warning_count
                + data.total.unsound_warning_count
                + data.total.yanked_warning_count;
            assert!(total_advisories > 0, "hyper should have historical advisories, got 0");

            assert!(data.timestamp.timestamp() > 0, "timestamp should be set");
        }
        other => panic!("Expected Found, got {other:?}"),
    }
}

#[tokio::test]
async fn test_advisory_provider_clean_crate() {
    let provider = shared_provider().await;

    // `itoa` is a tiny, well-maintained crate with no known advisories
    let crate_spec = make_spec("itoa", "1.0.14");
    let results: Vec<(CrateSpec, ProviderResult<AdvisoryData>)> = provider.get_advisory_data(vec![crate_spec]).await.collect();

    assert_eq!(results.len(), 1);
    let (spec, result) = &results[0];
    assert_eq!(spec.name(), "itoa");

    match result {
        ProviderResult::Found(data) => {
            let total = data.per_version.low_vulnerability_count
                + data.per_version.medium_vulnerability_count
                + data.per_version.high_vulnerability_count
                + data.per_version.critical_vulnerability_count
                + data.per_version.notice_warning_count
                + data.per_version.unmaintained_warning_count
                + data.per_version.unsound_warning_count
                + data.per_version.yanked_warning_count;
            assert_eq!(total, 0, "itoa 1.0.14 should have no advisories, got {total}");
        }
        other => panic!("Expected Found, got {other:?}"),
    }
}

#[tokio::test]
async fn test_advisory_provider_multiple_crates() {
    let provider = shared_provider().await;

    let crates = vec![
        make_spec("serde", "1.0.200"),
        make_spec("tokio", "1.37.0"),
        make_spec("hyper", "0.14.10"),
    ];

    let results: Vec<(CrateSpec, ProviderResult<AdvisoryData>)> = provider.get_advisory_data(crates).await.collect();

    // Should get a result for each input crate
    assert_eq!(results.len(), 3);

    // All results should be Found
    for (spec, result) in &results {
        assert!(
            matches!(result, ProviderResult::Found(_)),
            "Expected Found for {}, got {result:?}",
            spec.name()
        );
    }
}

#[tokio::test]
async fn test_advisory_provider_nonexistent_crate() {
    let provider = shared_provider().await;

    // A crate that doesn't exist in crates.io or the advisory DB
    let crate_spec = make_spec("this-crate-definitely-does-not-exist-xyz-98765", "0.0.1");
    let results: Vec<(CrateSpec, ProviderResult<AdvisoryData>)> = provider.get_advisory_data(vec![crate_spec]).await.collect();

    assert_eq!(results.len(), 1);
    let (_, result) = &results[0];

    // Advisory provider returns Found with zero counts for unknown crates
    // (it scans the DB, and simply finds no matching advisories)
    match result {
        ProviderResult::Found(data) => {
            let total = data.total.low_vulnerability_count
                + data.total.medium_vulnerability_count
                + data.total.high_vulnerability_count
                + data.total.critical_vulnerability_count
                + data.total.notice_warning_count
                + data.total.unmaintained_warning_count
                + data.total.unsound_warning_count
                + data.total.yanked_warning_count;
            assert_eq!(total, 0, "Non-existent crate should have no advisories");
        }
        other => panic!("Expected Found with zero counts, got {other:?}"),
    }
}

#[tokio::test]
async fn test_advisory_provider_empty_input() {
    let provider = shared_provider().await;

    assert!(
        provider.get_advisory_data(vec![]).await.next().is_none(),
        "Empty input should produce empty output"
    );
}

#[tokio::test]
async fn test_advisory_provider_historical_vs_per_version() {
    let provider = shared_provider().await;

    // Use a very old version of a crate known to have had advisories fixed in later versions
    // `smallvec` 0.6.13 has RUSTSEC-2019-0009 and RUSTSEC-2019-0012
    let crate_spec = make_spec("smallvec", "0.6.13");
    let results: Vec<(CrateSpec, ProviderResult<AdvisoryData>)> = provider.get_advisory_data(vec![crate_spec]).await.collect();

    assert_eq!(results.len(), 1);
    let (_, result) = &results[0];

    match result {
        ProviderResult::Found(data) => {
            // Historical (total) should have advisories
            let total_historical = data.total.low_vulnerability_count
                + data.total.medium_vulnerability_count
                + data.total.high_vulnerability_count
                + data.total.critical_vulnerability_count;
            assert!(total_historical > 0, "smallvec should have historical vulnerability advisories");

            // Per-version should also have advisories for this old version
            let total_per_version = data.per_version.low_vulnerability_count
                + data.per_version.medium_vulnerability_count
                + data.per_version.high_vulnerability_count
                + data.per_version.critical_vulnerability_count;
            assert!(
                total_per_version > 0,
                "smallvec 0.6.13 should have per-version vulnerability advisories"
            );
        }
        other => panic!("Expected Found, got {other:?}"),
    }
}

#[tokio::test]
async fn test_advisory_provider_cache_reuse() {
    // This test uses its own temp dir to verify the caching logic:
    // creating a second Provider from the same cache dir should not re-download.
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;

    // First creation downloads the database
    let provider1 = Provider::new(
        temp_dir.path(),
        core::time::Duration::from_secs(365 * 24 * 3600),
        Arc::clone(&progress),
        Utc::now(),
    )
    .await
    .expect("First provider creation should succeed");

    let results1: Vec<_> = provider1.get_advisory_data(vec![make_spec("hyper", "0.14.10")]).await.collect();

    // Second creation should reuse the cached database (no download)
    let provider2 = Provider::new(
        temp_dir.path(),
        core::time::Duration::from_secs(365 * 24 * 3600),
        progress,
        Utc::now(),
    )
    .await
    .expect("Second provider creation with cache should succeed");

    let results2: Vec<_> = provider2.get_advisory_data(vec![make_spec("hyper", "0.14.10")]).await.collect();

    // Both should return the same advisory counts
    assert_eq!(results1.len(), results2.len());
    match (&results1[0].1, &results2[0].1) {
        (ProviderResult::Found(d1), ProviderResult::Found(d2)) => {
            assert_eq!(
                d1.total.high_vulnerability_count, d2.total.high_vulnerability_count,
                "Cached results should match"
            );
            assert_eq!(
                d1.per_version.high_vulnerability_count, d2.per_version.high_vulnerability_count,
                "Cached per-version results should match"
            );
        }
        other => panic!("Expected Found for both, got {other:?}"),
    }
}
