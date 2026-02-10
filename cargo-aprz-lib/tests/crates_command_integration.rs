//! Integration test for the `crates` command.
//!
//! This test exercises the full end-to-end `crates` workflow: collect facts from
//! live data sources, flatten to metrics, and generate reports.
//!
//! Gated behind the `network_tests` feature:
//! ```sh
//! cargo test --features network_tests -p cargo-aprz-lib --test crates_command_integration
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

#[tokio::test]
async fn test_crates_command_all_report_types() {
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
            "crates",
            "serde@1.0.200",
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

    assert!(result.is_ok(), "crates command failed: {result:?}");

    // JSON report
    assert!(json_path.exists(), "JSON report file should be created");
    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");
    let crates = parsed["crates"].as_array().expect("crates array");
    assert_eq!(crates.len(), 1);
    let entry = &crates[0];
    assert_eq!(entry["name"].as_str(), Some("serde"));
    assert_eq!(entry["version"].as_str(), Some("1.0.200"));
    let metrics = entry["metrics"].as_object().expect("metrics object");
    assert!(!metrics.is_empty(), "should have metrics");

    // CSV report
    assert!(csv_path.exists(), "CSV report file should be created");
    let csv_content = std::fs::read_to_string(&csv_path).expect("read CSV");
    let csv_lines: Vec<&str> = csv_content.lines().collect();
    assert!(csv_lines.len() >= 2, "CSV should have header + data rows");
    assert!(csv_lines[0].starts_with("Metric"), "CSV header should start with 'Metric'");
    assert!(csv_content.contains("serde"), "CSV should contain crate name");

    // HTML report
    assert!(html_path.exists(), "HTML report file should be created");
    let html_content = std::fs::read_to_string(&html_path).expect("read HTML");
    assert!(html_content.contains("<html"), "HTML report should contain html tag");
    assert!(html_content.contains("serde"), "HTML report should mention crate name");

    // Excel report
    assert!(excel_path.exists(), "Excel report file should be created");
    let excel_size = std::fs::metadata(&excel_path).expect("excel metadata").len();
    assert!(excel_size > 0, "Excel report should not be empty");

    // Console output
    let console_output = host.output_str();
    assert!(console_output.contains("serde"), "console output should mention the crate name");
}

#[tokio::test]
async fn test_crates_command_csv_output() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let csv_path = temp_dir.path().join("report.csv");

    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "crates",
            "itoa@1.0.14",
            "--csv",
            csv_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "crates command failed: {result:?}");
    assert!(csv_path.exists(), "CSV report file should be created");

    let csv_content = std::fs::read_to_string(&csv_path).expect("read CSV");
    // CSV format: header row starts with "Metric", data rows follow
    let lines: Vec<&str> = csv_content.lines().collect();
    assert!(lines.len() >= 2, "CSV should have header + data, got {} lines", lines.len());
    assert!(
        lines[0].starts_with("Metric"),
        "first row should be the header starting with 'Metric'"
    );
    assert!(csv_content.contains("itoa"), "CSV should contain crate name");
}

#[tokio::test]
async fn test_crates_command_console_output() {
    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        ["cargo", "aprz", "crates", "serde@1.0.200", "--color", "never", "--console"],
    )
    .await;

    assert!(result.is_ok(), "crates command failed: {result:?}");

    let output = host.output_str();
    assert!(output.contains("serde"), "console output should mention the crate name");
}

#[tokio::test]
async fn test_crates_command_multiple_crates() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let json_path = temp_dir.path().join("report.json");

    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "crates",
            "serde@1.0.200",
            "itoa@1.0.14",
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    assert!(result.is_ok(), "crates command failed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    assert_eq!(crates.len(), 2, "should have 2 crate entries");

    let names: Vec<&str> = crates.iter().filter_map(|c| c["name"].as_str()).collect();
    assert!(names.contains(&"serde"), "should contain serde");
    assert!(names.contains(&"itoa"), "should contain itoa");
}

#[tokio::test]
async fn test_crates_command_nonexistent_crate() {
    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "crates",
            "this-crate-definitely-does-not-exist-xyz-98765@0.0.1",
            "--color",
            "never",
            "--console",
        ],
    )
    .await;

    // Should succeed (non-existent crates are reported, not fatal)
    assert!(result.is_ok(), "crates command should not fail for unknown crates: {result:?}");
}

// ---------------------------------------------------------------------------
// Line 245: CrateNotFound with non-empty suggestions ("Did you mean ...?")
// Uses a misspelled name close to a real crate so suggestions are returned.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_crates_command_misspelled_crate_shows_suggestions() {
    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        ["cargo", "aprz", "crates", "serdee@1.0.0", "--color", "never", "--console"],
    )
    .await;

    // The command itself succeeds; the crate is reported as not found
    assert!(result.is_ok(), "should not fail: {result:?}");

    let error_output = host.error_str();
    assert!(
        error_output.contains("Did you mean"),
        "error output should contain suggestions, got: {error_output}"
    );
}

// ---------------------------------------------------------------------------
// Line 262: VersionNotFound — real crate with a nonexistent version
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_crates_command_nonexistent_version() {
    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        ["cargo", "aprz", "crates", "serde@99.99.99", "--color", "never", "--console"],
    )
    .await;

    assert!(result.is_ok(), "should not fail: {result:?}");

    let error_output = host.error_str();
    assert!(
        error_output.contains("Could not find information on version"),
        "error output should mention missing version, got: {error_output}"
    );
    assert!(
        error_output.contains("serde"),
        "error output should mention the crate name, got: {error_output}"
    );
}

// ---------------------------------------------------------------------------
// Line 310: should_eval branch — --check triggers expression evaluation
// and produces ReportableCrate with an evaluation result.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_crates_command_with_check_flag() {
    let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
    let json_path = temp_dir.path().join("report.json");

    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "crates",
            "serde@1.0.200",
            "--check",
            "--json",
            json_path.to_str().expect("valid path"),
            "--color",
            "never",
        ],
    )
    .await;

    // --check with no expressions means evaluation succeeds (nothing to deny)
    assert!(result.is_ok(), "crates --check should succeed: {result:?}");

    let json_content = std::fs::read_to_string(&json_path).expect("read JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json_content).expect("valid JSON");

    let crates = parsed["crates"].as_array().expect("crates array");
    assert_eq!(crates.len(), 1);
    assert_eq!(crates[0]["name"].as_str(), Some("serde"));

    // With --check, the evaluation field should be present
    let eval = &crates[0]["evaluation"];
    assert!(!eval.is_null(), "evaluation should be present when --check is used");
}
