//! Integration tests for the docs provider using real fixtures and wiremock

use cargo_aprz_lib::facts::docs::{DocMetricState, DocsData, DocsMetrics, Provider};
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
#[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
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
    let provider = Provider::new(temp_dir.path(), Utc::now(), false, Some(&mock_server.uri()));

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
#[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
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
    let provider = Provider::new(temp_dir.path(), Utc::now(), false, Some(&mock_server.uri()));

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

/// Helper to create a sentinel `DocsData` for cache tests
fn make_sentinel_docs_data() -> DocsData {
    DocsData {
        timestamp: Utc::now(),
        metrics: DocMetricState::Found(DocsMetrics {
            doc_coverage_percentage: 42.0,
            public_api_elements: 100,
            undocumented_elements: 58,
            examples_in_docs: 5,
            has_crate_level_docs: true,
            broken_doc_links: 0,
        }),
    }
}

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
async fn test_docs_provider_uses_cache() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Pre-populate cache with sentinel data
    let cached_data = make_sentinel_docs_data();
    let cache_path = temp_dir.path().join("anyhow@1.0.100.json");
    let json = serde_json::to_string(&cached_data).expect("serialize");
    fs::write(&cache_path, json).expect("write cache file");

    // Create provider with ignore_cached=false and no mock server (would fail if it tried to fetch)
    let mock_server = MockServer::start().await;
    let provider = Provider::new(temp_dir.path(), Utc::now(), false, Some(&mock_server.uri()));

    let crate_spec = CrateSpec::from_arcs(Arc::from("anyhow"), Arc::new(Version::parse("1.0.100").unwrap()));
    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let tracker = RequestTracker::new(progress.as_ref());
    let results: Vec<_> = provider.get_docs_data(vec![crate_spec], &tracker).await.collect();

    assert_eq!(results.len(), 1);
    match &results[0].1 {
        ProviderResult::Found(data) => {
            // Verify we got the sentinel cached data back
            if let DocMetricState::Found(metrics) = &data.metrics {
                assert!((metrics.doc_coverage_percentage - 42.0).abs() < f64::EPSILON);
            } else {
                panic!("Expected Found metrics from cache");
            }
        }
        other => panic!("Expected Found, got {other:?}"),
    }

    // Verify no requests were made to the server
    let requests = mock_server.received_requests().await.unwrap();
    assert!(requests.is_empty(), "Expected no HTTP requests when using cache");
}

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
async fn test_docs_provider_ignore_cached_bypasses_cache() {
    if !fixture_exists() {
        eprintln!("Skipping test: fixture file {FIXTURE_PATH} not found");
        return;
    }

    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Pre-populate cache with sentinel data
    let cached_data = make_sentinel_docs_data();
    let cache_path = temp_dir.path().join("anyhow@1.0.100.json");
    let json = serde_json::to_string(&cached_data).expect("serialize");
    fs::write(&cache_path, json).expect("write cache file");

    // Set up mock server with real fixture
    let zst_data = fs::read(FIXTURE_PATH).expect("Failed to read fixture file");
    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/crate/anyhow/1.0.100/json"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(zst_data)
                .insert_header("content-type", "application/zstd"),
        )
        .mount(&mock_server)
        .await;

    // Create provider with ignore_cached=true
    let provider = Provider::new(temp_dir.path(), Utc::now(), true, Some(&mock_server.uri()));

    let crate_spec = CrateSpec::from_arcs(Arc::from("anyhow"), Arc::new(Version::parse("1.0.100").unwrap()));
    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let tracker = RequestTracker::new(progress.as_ref());
    let results: Vec<_> = provider.get_docs_data(vec![crate_spec], &tracker).await.collect();

    assert_eq!(results.len(), 1);
    match &results[0].1 {
        ProviderResult::Found(data) => {
            // Verify we got fresh data, not the sentinel
            if let DocMetricState::Found(metrics) = &data.metrics {
                assert!(
                    (metrics.doc_coverage_percentage - 42.0).abs() > f64::EPSILON,
                    "Expected fresh data different from sentinel, got {}",
                    metrics.doc_coverage_percentage
                );
            } else {
                panic!("Expected Found metrics from fresh fetch");
            }
        }
        other => panic!("Expected Found, got {other:?}"),
    }

    // Verify a request WAS made to the server
    let requests = mock_server.received_requests().await.unwrap();
    assert!(!requests.is_empty(), "Expected HTTP request when ignore_cached=true");
}
