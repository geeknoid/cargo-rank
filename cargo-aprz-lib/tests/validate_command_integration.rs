//! Integration test for the `validate` command.
//!
//! Exercises the path where no explicit `--config` is provided, causing
//! `validate_config` to resolve the workspace root via `MetadataCommand`
//! (line 59 of validate.rs).
//!
//! This test does NOT require the `network_tests` feature because it only
//! exercises local config validation logic (no network access).

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

/// Validate without `--config` so the workspace root is resolved via
/// `MetadataCommand` (covers validate.rs line 59).
#[tokio::test]
#[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
async fn test_validate_without_explicit_config() {
    let mut host = TestHost::new();
    let result = cargo_aprz_lib::run(
        &mut host,
        [
            "cargo",
            "aprz",
            "validate",
            "--manifest-path",
            "tests/fixtures/tiny-crate/Cargo.toml",
        ],
    )
    .await;

    assert!(result.is_ok(), "validate without --config should succeed: {result:?}");

    let output = host.output_str();
    // Without --config, no explicit config path is printed; the second writeln
    // overwrites the first in the Cursor-based TestHost, so we only see the last line.
    assert!(
        output.contains("default configuration"),
        "should mention default configuration, got: {output}"
    );
}
