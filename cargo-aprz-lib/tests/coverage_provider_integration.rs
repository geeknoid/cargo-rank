//! Integration tests for the coverage provider using real fixtures and wiremock

use cargo_aprz_lib::facts::coverage::{CoverageData, Provider};
use cargo_aprz_lib::facts::{CrateSpec, Progress, ProviderResult, RepoSpec, RequestTracker};
use chrono::Utc;
use semver::Version;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use url::Url;
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

const FIXTURE_PATH: &str = "tests/fixtures/microsoft-oxidizer-coverage.svg";

/// Helper to check if fixture file exists
fn fixture_exists() -> bool {
    Path::new(FIXTURE_PATH).exists()
}

#[tokio::test]
async fn test_coverage_provider_with_fixture() {
    // Skip test if fixture doesn't exist
    if !fixture_exists() {
        eprintln!("Skipping test: fixture file {FIXTURE_PATH} not found");
        return;
    }

    // Read the fixture file
    let svg_data = fs::read(FIXTURE_PATH).expect("Failed to read fixture file");

    // Start wiremock server
    let mock_server = MockServer::start().await;

    // Set up mock to return the SVG for main branch
    Mock::given(method("GET"))
        .and(path("/gh/microsoft/oxidizer/branch/main/graph/badge.svg"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_bytes(svg_data.clone())
                .insert_header("content-type", "image/svg+xml"),
        )
        .mount(&mock_server)
        .await;

    // Create a temporary directory for the cache
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create provider with mock server URL
    let provider = Provider::new(
        temp_dir.path(),
        core::time::Duration::from_secs(365 * 24 * 3600), // 1 year TTL
        Utc::now(),
        Some(&mock_server.uri()),
    );

    // Create crate spec with repository
    let repo_url = Url::parse("https://github.com/microsoft/oxidizer").expect("Failed to parse repo URL");
    let repo_spec = RepoSpec::parse(repo_url).expect("Failed to create repo spec");
    let crate_spec = CrateSpec::from_arcs_with_repo(Arc::from("oxidizer"), Arc::new(Version::parse("1.0.0").unwrap()), repo_spec);

    // Fetch coverage data
    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let tracker = RequestTracker::new(progress.as_ref());
    let results: Vec<(CrateSpec, ProviderResult<CoverageData>)> =
        provider.get_coverage_data(vec![crate_spec.clone()], &tracker).await.collect();

    // Verify results
    assert_eq!(results.len(), 1);
    let (result_spec, result_data) = &results[0];
    assert_eq!(result_spec.name(), "oxidizer");

    // Verify we got valid coverage data
    match result_data {
        ProviderResult::Found(coverage_data) => {
            eprintln!(
                "Successfully parsed coverage for microsoft/oxidizer: {}%",
                coverage_data.code_coverage_percentage
            );

            // The fixture SVG contains 100% coverage
            assert!(
                (coverage_data.code_coverage_percentage - 100.0).abs() < 0.01,
                "Expected 100% coverage, got {}%",
                coverage_data.code_coverage_percentage
            );

            assert!(coverage_data.timestamp.timestamp() > 0);
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
async fn test_coverage_provider_not_found_main() {
    // Start wiremock server
    let mock_server = MockServer::start().await;

    // Set up mock to return 404 for main branch
    Mock::given(method("GET"))
        .and(path("/gh/nonexistent/repo/branch/main/graph/badge.svg"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    // Set up mock to return 404 for master branch
    Mock::given(method("GET"))
        .and(path("/gh/nonexistent/repo/branch/master/graph/badge.svg"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    // Create a temporary directory for the cache
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create provider with mock server URL
    let provider = Provider::new(
        temp_dir.path(),
        core::time::Duration::from_secs(365 * 24 * 3600),
        Utc::now(),
        Some(&mock_server.uri()),
    );

    // Create crate spec for nonexistent repo
    let repo_url = Url::parse("https://github.com/nonexistent/repo").expect("Failed to parse repo URL");
    let repo_spec = RepoSpec::parse(repo_url).expect("Failed to create repo spec");
    let crate_spec = CrateSpec::from_arcs_with_repo(Arc::from("nonexistent"), Arc::new(Version::parse("1.0.0").unwrap()), repo_spec);

    // Fetch coverage data
    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let tracker = RequestTracker::new(progress.as_ref());
    let results: Vec<(CrateSpec, ProviderResult<CoverageData>)> = provider.get_coverage_data(vec![crate_spec], &tracker).await.collect();

    // Verify results
    assert_eq!(results.len(), 1);
    let (_, result_data) = &results[0];

    // Should be CrateNotFound when both main and master return 404
    assert!(matches!(result_data, ProviderResult::CrateNotFound(_)));
}

#[tokio::test]
async fn test_coverage_provider_unknown_coverage() {
    // Skip test if fixture doesn't exist
    if !fixture_exists() {
        eprintln!("Skipping test: fixture file {FIXTURE_PATH} not found");
        return;
    }

    // Start wiremock server
    let mock_server = MockServer::start().await;

    // Create SVG with "unknown" coverage
    let unknown_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="122" height="20">
        <text x="98" y="14">unknown</text>
    </svg>"#;

    // Set up mock to return the unknown SVG
    Mock::given(method("GET"))
        .and(path("/gh/test/repo/branch/main/graph/badge.svg"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(unknown_svg)
                .insert_header("content-type", "image/svg+xml"),
        )
        .mount(&mock_server)
        .await;

    // Create a temporary directory for the cache
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

    // Create provider with mock server URL
    let provider = Provider::new(
        temp_dir.path(),
        core::time::Duration::from_secs(365 * 24 * 3600),
        Utc::now(),
        Some(&mock_server.uri()),
    );

    // Create crate spec with repository
    let repo_url = Url::parse("https://github.com/test/repo").expect("Failed to parse repo URL");
    let repo_spec = RepoSpec::parse(repo_url).expect("Failed to create repo spec");
    let crate_spec = CrateSpec::from_arcs_with_repo(Arc::from("testrepo"), Arc::new(Version::parse("1.0.0").unwrap()), repo_spec);

    // Fetch coverage data
    let progress = Arc::new(NoOpProgress) as Arc<dyn Progress>;
    let tracker = RequestTracker::new(progress.as_ref());
    let results: Vec<(CrateSpec, ProviderResult<CoverageData>)> = provider.get_coverage_data(vec![crate_spec], &tracker).await.collect();

    // Verify results - should be CrateNotFound when coverage is unknown
    assert_eq!(results.len(), 1);
    let (_, result_data) = &results[0];
    assert!(matches!(result_data, ProviderResult::CrateNotFound(_)));
}
