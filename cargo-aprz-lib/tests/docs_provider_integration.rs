//! Integration tests for the docs provider using real fixtures and wiremock

use cargo_aprz_lib::facts::docs::{DocsData, Provider};
use cargo_aprz_lib::facts::{CrateSpec, Progress, ProviderResult, RequestTracker};
use chrono::Utc;
use semver::Version;
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

const FIXTURE_PATH: &str = "tests/fixtures/anyhow-1.0.100.json.zst";

/// Helper to check if fixture file exists
fn fixture_exists() -> bool {
    Path::new(FIXTURE_PATH).exists()
}

#[tokio::test]
async fn test_docs_provider_with_fixture() {
    // Skip test if fixture doesn't exist
    if !fixture_exists() {
        eprintln!("Skipping test: fixture file {FIXTURE_PATH} not found");
        return;
    }

    // Read the fixture file
    let zst_data = fs::read(FIXTURE_PATH).expect("Failed to read fixture file");

    // Start wiremock server
    let mock_server = MockServer::start().await;

    // Set up mock to return the zst file
    Mock::given(method("GET"))
        .and(path("/crate/anyhow/1.0.100/json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(zst_data)
                .insert_header("content-type", "application/zstd"),
        )
        .mount(&mock_server)
        .await;

    // Create a temporary directory for the cache
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create provider with mock server URL
    let provider = Provider::new(temp_dir.path(), Utc::now(), Some(&mock_server.uri()));

    // Create crate spec for anyhow 1.0.100
    let crate_spec = CrateSpec::from_arcs(Arc::from("anyhow"), Arc::new(Version::parse("1.0.100").unwrap()));

    // Fetch docs data
    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let tracker = RequestTracker::new(progress.as_ref());
    let results: Vec<(CrateSpec, ProviderResult<DocsData>)> = provider.get_docs_data(vec![crate_spec.clone()], &tracker).await.collect();

    // Verify results
    assert_eq!(results.len(), 1);
    let (result_spec, result_data) = &results[0];
    assert_eq!(result_spec.name(), "anyhow");
    assert_eq!(result_spec.version().to_string(), "1.0.100");

    // Verify we got valid docs data
    match result_data {
        ProviderResult::Found(docs_data) => {
            // Verify basic structure exists
            assert!(docs_data.timestamp.timestamp() > 0);
            // The metrics should be parseable (not an error state)
            eprintln!("Successfully parsed docs for anyhow 1.0.100: {:?}", docs_data.metrics);
        }
        ProviderResult::Error(e) => {
            panic!("Expected Found result, got Error: {e:#}");
        }
        ProviderResult::CrateNotFound(_) => {
            panic!("Expected Found result, got CrateNotFound");
        }
        ProviderResult::VersionNotFound => {
            panic!("Expected Found result, got VersionNotFound");
        }
    }
}

#[tokio::test]
async fn test_docs_provider_not_found() {
    // Start wiremock server
    let mock_server = MockServer::start().await;

    // Set up mock to return 404
    Mock::given(method("GET"))
        .and(path("/crate/nonexistent/1.0.0/json"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    // Create a temporary directory for the cache
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create provider with mock server URL
    let provider = Provider::new(temp_dir.path(), Utc::now(), Some(&mock_server.uri()));

    // Create crate spec for nonexistent crate
    let crate_spec = CrateSpec::from_arcs(Arc::from("nonexistent"), Arc::new(Version::parse("1.0.0").unwrap()));

    // Fetch docs data
    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let tracker = RequestTracker::new(progress.as_ref());
    let results: Vec<(CrateSpec, ProviderResult<DocsData>)> = provider.get_docs_data(vec![crate_spec], &tracker).await.collect();

    // Verify results
    assert_eq!(results.len(), 1);
    let (_, result_data) = &results[0];

    // Should be CrateNotFound
    assert!(matches!(result_data, ProviderResult::CrateNotFound(_)));
}
