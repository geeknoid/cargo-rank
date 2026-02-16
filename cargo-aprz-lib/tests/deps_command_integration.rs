//! Integration test for the `deps` command.
//!
//! Uses tiny fixture crates to keep network traffic to a minimum:
//! - `tests/fixtures/tiny-crate`: single-package workspace with `itoa` + `miniz_oxide` (→ `adler2`)
//! - `tests/fixtures/tiny-virtual-workspace`: virtual workspace with one member depending on `itoa`
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
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
async fn test_deps_command_all_report_types() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let json_path = temp_dir.path().join("report.json");
    let csv_path = temp_dir.path().join("report.csv");
    let html_path = temp_dir.path().join("report.html");
    let excel_path = temp_dir.path().join("report.xlsx");

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
            "--csv",
            csv_path.to_str().expect("valid path"),
            "--html",
            html_path.to_str().expect("valid path"),
            "--excel",
            excel_path.to_str().expect("valid path"),
            "--console",
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps command failed: {result:?}");

    // JSON report
    assert!(json_path.exists(), "JSON report should be created");
    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");
    let crates = parsed["crates"].as_array().expect("crates array");
    assert_eq!(
        crates.len(),
        4,
        "tiny-crate should have 4 dependencies (itoa, miniz_oxide, adler2, once_cell)"
    );
    let names: Vec<&str> = crates.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names.contains(&"itoa"), "should contain itoa");
    assert!(names.contains(&"miniz_oxide"), "should contain miniz_oxide");
    assert!(names.contains(&"adler2"), "should contain adler2 (transitive dep)");
    assert!(names.contains(&"once_cell"), "should contain once_cell (default feature dep)");
    let entry = crates.iter().find(|c| c["name"].as_str() == Some("itoa")).expect("itoa entry");
    let metrics = entry["metrics"].as_object().expect("metrics object");
    assert!(!metrics.is_empty(), "should have metrics");

    // CSV report
    assert!(csv_path.exists(), "CSV report should be created");
    let csv_content = std::fs::read_to_string(&csv_path).expect("read CSV");
    let csv_lines: Vec<&str> = csv_content.lines().collect();
    assert!(csv_lines.len() >= 2, "CSV should have header + data rows");
    assert!(csv_lines[0].starts_with("Metric"), "CSV header should start with 'Metric'");
    assert!(csv_content.contains("itoa"), "CSV should contain itoa");

    // HTML report
    assert!(html_path.exists(), "HTML report should be created");
    let html_content = std::fs::read_to_string(&html_path).expect("read HTML");
    assert!(html_content.contains("<html"), "HTML report should contain html tag");
    assert!(html_content.contains("itoa"), "HTML report should mention itoa");

    // Excel report
    assert!(excel_path.exists(), "Excel report should be created");
    let excel_size = std::fs::metadata(&excel_path).expect("excel metadata").len();
    assert!(excel_size > 0, "Excel report should not be empty");

    // Console output
    let console_output = host.output_str();
    assert!(console_output.contains("itoa"), "console output should mention itoa");
    assert!(console_output.contains("adler2"), "console output should mention adler2");
}

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
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
    assert!(output.contains("adler2"), "console output should mention adler2 (transitive dep)");
}

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
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
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
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
    // itoa, miniz_oxide, adler2, and once_cell are all standard dependencies
    assert_eq!(crates.len(), 4);
    let names: Vec<&str> = crates.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names.contains(&"itoa"));
    assert!(names.contains(&"miniz_oxide"));
    assert!(names.contains(&"adler2"));
    assert!(names.contains(&"once_cell"));
}

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
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
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
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

// ---------------------------------------------------------------------------
// Line 95: --package <name> filters to a specific package in the workspace
// ---------------------------------------------------------------------------

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
async fn test_deps_command_with_package_flag() {
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
            "--package",
            "tiny-crate",
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps --package tiny-crate should succeed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    assert_eq!(crates.len(), 4, "should still resolve all 4 deps for tiny-crate");
}

// ---------------------------------------------------------------------------
// Line 108: --workspace processes all workspace members
// ---------------------------------------------------------------------------

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
async fn test_deps_command_with_workspace_flag() {
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
            "--workspace",
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps --workspace should succeed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    assert_eq!(crates.len(), 4, "workspace should resolve all 4 deps");
}

// ---------------------------------------------------------------------------
// Line 120: virtual workspace (no root package, no --package, no --workspace)
// defaults to processing all workspace members
// ---------------------------------------------------------------------------

const VIRTUAL_WS_MANIFEST: &str = "tests/fixtures/tiny-virtual-workspace/Cargo.toml";

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
async fn test_deps_command_virtual_workspace() {
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
            VIRTUAL_WS_MANIFEST,
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps on virtual workspace should succeed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    // The virtual workspace member depends only on itoa
    assert_eq!(crates.len(), 1);
    assert_eq!(crates[0]["name"].as_str(), Some("itoa"));
}

// ---------------------------------------------------------------------------
// Lines 60-61: --all-features activates all features
// ---------------------------------------------------------------------------

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
async fn test_deps_command_all_features() {
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
            "--all-features",
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps --all-features should succeed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    let names: Vec<&str> = crates.iter().filter_map(|c| c["name"].as_str()).collect();
    // --all-features should include once_cell (behind "extra" feature)
    assert!(names.contains(&"once_cell"), "should contain once_cell with --all-features");
    assert!(names.contains(&"itoa"), "should contain itoa");
    assert_eq!(crates.len(), 4);
}

// ---------------------------------------------------------------------------
// Lines 63-64: --no-default-features disables the default feature set
// ---------------------------------------------------------------------------

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
async fn test_deps_command_no_default_features() {
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
            "--no-default-features",
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps --no-default-features should succeed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    let names: Vec<&str> = crates.iter().filter_map(|c| c["name"].as_str()).collect();
    // --no-default-features should exclude once_cell (only in "extra" default feature)
    assert!(
        !names.contains(&"once_cell"),
        "should not contain once_cell with --no-default-features"
    );
    assert!(names.contains(&"itoa"), "should contain itoa");
    assert_eq!(crates.len(), 3);
}

// ---------------------------------------------------------------------------
// Lines 67-68: --features <list> activates specific features
// ---------------------------------------------------------------------------

#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call mkdir")]
async fn test_deps_command_explicit_features() {
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
            "--no-default-features",
            "-F",
            "extra",
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "deps -F extra should succeed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    let names: Vec<&str> = crates.iter().filter_map(|c| c["name"].as_str()).collect();
    // --no-default-features -F extra should re-enable once_cell
    assert!(names.contains(&"once_cell"), "should contain once_cell with explicit -F extra");
    assert!(names.contains(&"itoa"), "should contain itoa");
    assert_eq!(crates.len(), 4);
}
