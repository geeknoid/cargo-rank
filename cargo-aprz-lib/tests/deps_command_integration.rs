//! Integration test for the `deps` command.
//!
//! Uses a tiny fixture crate (`tests/fixtures/tiny-crate`) whose only dependency
//! is `itoa`, keeping network traffic to a minimum.
//!
//! Gated behind the `network_tests` feature:
//! ```sh
//! cargo test --features network_tests -p cargo-aprz-lib --test deps_command_integration
//! ```

#![cfg(feature = "network_tests")]

use cargo_aprz_lib::Host;
use std::io::Cursor;

/// Test host that captures output to in-memory buffers.
struct TestHost {
    output_buf: Vec<u8>,
    error_buf: Vec<u8>,
}

impl TestHost {
    const fn new() -> Self {
        Self {
            output_buf: Vec::new(),
            error_buf: Vec::new(),
        }
    }

    fn output_str(&self) -> String {
        String::from_utf8_lossy(&self.output_buf).into_owned()
    }

    fn error_str(&self) -> String {
        String::from_utf8_lossy(&self.error_buf).into_owned()
    }
}

impl Host for TestHost {
    fn output(&mut self) -> impl std::io::Write {
        Cursor::new(&mut self.output_buf)
    }

    fn error(&mut self) -> impl std::io::Write {
        Cursor::new(&mut self.error_buf)
    }

    fn exit(&mut self, _code: i32) {}
}

/// Path to the fixture crate's Cargo.toml, relative to `cargo-aprz-lib/`.
const FIXTURE_MANIFEST: &str = "tests/fixtures/tiny-crate/Cargo.toml";

#[tokio::test]
async fn test_deps_command_json_output() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let json_path = temp_dir.path().join("report.json");

    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "deps",
            "--manifest-path",
            FIXTURE_MANIFEST,
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps command failed: {result:?}");
    assert!(json_path.exists(), "JSON report should be created");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    assert_eq!(crates.len(), 1, "tiny-crate has exactly one dependency");

    let entry = &crates[0];
    assert_eq!(entry["name"].as_str(), Some("itoa"));

    let metrics = entry["metrics"].as_object().expect("metrics object");
    assert!(!metrics.is_empty(), "should have metrics");
}

#[tokio::test]
async fn test_deps_command_console_output() {
    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "deps",
            "--manifest-path",
            FIXTURE_MANIFEST,
            "--color",
            "never",
            "--console",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps command failed: {result:?}");

    let output = host.output_str();
    assert!(output.contains("itoa"), "console output should mention itoa");
}

#[tokio::test]
async fn test_deps_command_csv_output() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let csv_path = temp_dir.path().join("report.csv");

    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "deps",
            "--manifest-path",
            FIXTURE_MANIFEST,
            "--csv",
            csv_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps command failed: {result:?}");
    assert!(csv_path.exists(), "CSV report should be created");

    let csv_content = std::fs::read_to_string(&csv_path).expect("read CSV");
    let lines: Vec<&str> = csv_content.lines().collect();
    assert!(lines.len() >= 2, "CSV should have header + data, got {} lines", lines.len());
    assert!(lines[0].starts_with("Metric"), "header row should start with 'Metric'");
    assert!(csv_content.contains("itoa"), "CSV should contain itoa");
}

#[tokio::test]
async fn test_deps_command_standard_deps_only() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let json_path = temp_dir.path().join("report.json");

    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "deps",
            "--manifest-path",
            FIXTURE_MANIFEST,
            "--dependency-types",
            "standard",
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps command failed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    // itoa is a standard dependency
    assert_eq!(crates.len(), 1);
    assert_eq!(crates[0]["name"].as_str(), Some("itoa"));
}

#[tokio::test]
async fn test_deps_command_dev_deps_only() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let json_path = temp_dir.path().join("report.json");

    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "deps",
            "--manifest-path",
            FIXTURE_MANIFEST,
            "--dependency-types",
            "dev",
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps command failed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    // tiny-crate has no dev dependencies
    assert!(
        crates.is_empty(),
        "tiny-crate should have no dev dependencies, got {}",
        crates.len()
    );
}

#[tokio::test]
async fn test_deps_command_nonexistent_package() {
    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "deps",
            "--manifest-path",
            FIXTURE_MANIFEST,
            "--package",
            "no-such-package",
            "--color",
            "never",
            "--console",
        ],
    )
    .await;

    assert!(result.is_err(), "should fail for nonexistent package");
    let err_msg = host.error_str();
    let result_msg = format!("{}", result.unwrap_err());
    let has_error = err_msg.contains("no-such-package") || result_msg.contains("no-such-package");
    assert!(has_error, "error should mention the package name");
}
