//! Integration tests for the crates provider using real fixtures and wiremock

use cargo_aprz_lib::facts::crates::Provider;
use cargo_aprz_lib::facts::{CrateRef, Progress, ProviderResult};
use chrono::Utc;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// No-op progress reporter for testing
#[derive(Debug)]
struct NoOpProgress;

impl Progress for NoOpProgress {
    fn set_phase(&self, _phase: &str) {}
    fn set_determinate(&self, _callback: Box<dyn Fn() -> (u64, u64, String) + Send + Sync + 'static>) {}
    fn set_indeterminate(&self, _callback: Box<dyn Fn() -> String + Send + Sync + 'static>) {}
    fn done(&self) {}
}

const FIXTURE_PATH: &str = "tests/fixtures/db-dump.tar.gz";

/// Helper to check if fixture file exists
fn fixture_exists() -> bool {
    Path::new(FIXTURE_PATH).exists()
}

#[tokio::test]
async fn test_crates_provider_with_fixture() {
    // Skip test if fixture doesn't exist
    if !fixture_exists() {
        eprintln!("Skipping test: fixture file {FIXTURE_PATH} not found");
        return;
    }

    // Read the fixture file
    let tarball_data = fs::read(FIXTURE_PATH).expect("Failed to read fixture file");

    // Start wiremock server
    let mock_server = MockServer::start().await;

    // Set up mock to return the tarball
    Mock::given(method("GET"))
        .and(path("/db-dump.tar.gz"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(tarball_data)
                .insert_header("content-type", "application/gzip"),
        )
        .mount(&mock_server)
        .await;

    // Create a temporary directory for the cache
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create provider with mock server URL
    let mock_url = format!("{}/db-dump.tar.gz", mock_server.uri());
    let provider = Provider::new(
        temp_dir.path(),
        core::time::Duration::from_secs(365 * 24 * 3600), // 1 year TTL
        Arc::new(NoOpProgress) as Arc<dyn Progress>,
        Utc::now(),
        Some(&mock_url),
    )
    .await
    .expect("Failed to create provider");

    // Test fetching data for a well-known crate (serde should be in the dump)
    let crate_refs = vec![CrateRef::new("serde", None), CrateRef::new("tokio", None)];

    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let results: Vec<_> = provider.get_crates_data(crate_refs, progress.as_ref(), false).await.collect();

    // Verify we got results for both crates
    assert_eq!(results.len(), 2);

    for (crate_spec, result) in results {
        match result {
            ProviderResult::Found(crate_data) => {
                eprintln!("Found crate: {} version {}", crate_spec.name(), crate_spec.version());
                eprintln!("  Repository: {:?}", crate_data.overall_data.repository);
                eprintln!("  Downloads: {}", crate_data.overall_data.downloads);
                eprintln!("  Categories: {:?}", crate_data.overall_data.categories);
                eprintln!("  Keywords: {:?}", crate_data.overall_data.keywords);

                // Verify basic sanity checks
                assert!(
                    crate_data.overall_data.downloads > 0,
                    "Crate {} should have downloads",
                    crate_spec.name()
                );
                assert!(
                    crate_data.overall_data.repository.is_some(),
                    "Crate {} should have a repository",
                    crate_spec.name()
                );
            }
            ProviderResult::Error(e) => {
                panic!("Expected Found result for {}, got Error: {:#}", crate_spec.name(), e);
            }
            ProviderResult::CrateNotFound(suggestions) => {
                panic!(
                    "Expected Found result for {}, got CrateNotFound with suggestions: {:?}",
                    crate_spec.name(),
                    suggestions
                );
            }
            ProviderResult::VersionNotFound => {
                panic!("Expected Found result for {}, got VersionNotFound", crate_spec.name());
            }
        }
    }
}

#[tokio::test]
async fn test_crates_provider_nonexistent_crate() {
    // Skip test if fixture doesn't exist
    if !fixture_exists() {
        eprintln!("Skipping test: fixture file {FIXTURE_PATH} not found");
        return;
    }

    // Read the fixture file
    let tarball_data = fs::read(FIXTURE_PATH).expect("Failed to read fixture file");

    // Start wiremock server
    let mock_server = MockServer::start().await;

    // Set up mock to return the tarball
    Mock::given(method("GET"))
        .and(path("/db-dump.tar.gz"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(tarball_data)
                .insert_header("content-type", "application/gzip"),
        )
        .mount(&mock_server)
        .await;

    // Create a temporary directory for the cache
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create provider with mock server URL
    let mock_url = format!("{}/db-dump.tar.gz", mock_server.uri());
    let provider = Provider::new(
        temp_dir.path(),
        core::time::Duration::from_secs(365 * 24 * 3600),
        Arc::new(NoOpProgress) as Arc<dyn Progress>,
        Utc::now(),
        Some(&mock_url),
    )
    .await
    .expect("Failed to create provider");

    // Test with a nonexistent crate
    let crate_refs = vec![CrateRef::new("this-crate-definitely-does-not-exist-12345", None)];

    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let results: Vec<_> = provider.get_crates_data(crate_refs, progress.as_ref(), true).await.collect();

    // Verify we got a not found result
    assert_eq!(results.len(), 1);
    let (_, result) = &results[0];

    assert!(
        matches!(result, ProviderResult::CrateNotFound(_)),
        "Expected CrateNotFound, got {result:?}"
    );
}

#[tokio::test]
async fn test_crates_provider_with_suggestions() {
    // Skip test if fixture doesn't exist
    if !fixture_exists() {
        eprintln!("Skipping test: fixture file {FIXTURE_PATH} not found");
        return;
    }

    // Read the fixture file
    let tarball_data = fs::read(FIXTURE_PATH).expect("Failed to read fixture file");

    // Start wiremock server
    let mock_server = MockServer::start().await;

    // Set up mock to return the tarball
    Mock::given(method("GET"))
        .and(path("/db-dump.tar.gz"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(tarball_data)
                .insert_header("content-type", "application/gzip"),
        )
        .mount(&mock_server)
        .await;

    // Create a temporary directory for the cache
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create provider with mock server URL
    let mock_url = format!("{}/db-dump.tar.gz", mock_server.uri());
    let provider = Provider::new(
        temp_dir.path(),
        core::time::Duration::from_secs(365 * 24 * 3600),
        Arc::new(NoOpProgress) as Arc<dyn Progress>,
        Utc::now(),
        Some(&mock_url),
    )
    .await
    .expect("Failed to create provider");

    // Test with a typo in a well-known crate name (should get suggestions)
    let crate_refs = vec![CrateRef::new("tokioo", None)]; // typo in "tokio"

    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let results: Vec<_> = provider.get_crates_data(crate_refs, progress.as_ref(), true).await.collect();

    // Verify we got suggestions
    assert_eq!(results.len(), 1);
    let (crate_spec, result) = &results[0];

    match result {
        ProviderResult::CrateNotFound(suggestions) => {
            eprintln!("Crate '{}' not found, suggestions: {:?}", crate_spec.name(), suggestions);
            // Should have at least one suggestion
            assert!(!suggestions.is_empty(), "Expected suggestions for typo 'tokioo'");
        }
        _ => panic!("Expected CrateNotFound with suggestions, got {result:?}"),
    }
}
