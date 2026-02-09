use super::Host;
use super::config::Config;
use crate::Result;
use crate::expr::evaluate;
use crate::metrics::default_metrics;
use camino::{Utf8Path, Utf8PathBuf};
use chrono::Local;
use clap::Parser;
use ohno::IntoAppError;
use std::io::Write;

#[derive(Parser, Debug)]
pub struct ValidateArgs {
    /// Path to configuration file (default is `aprz.toml`)
    #[arg(long, short = 'c', value_name = "PATH")]
    pub config: Option<Utf8PathBuf>,
}

/// Validates a configuration file by loading it and checking expression evaluation
///
/// # Errors
///
/// Returns an error if the config file cannot be loaded, parsed, or if expressions fail to evaluate
fn validate_config_inner(workspace_root: &Utf8Path, config_path: Option<&Utf8PathBuf>) -> Result<()> {
    let config = Config::load(workspace_root, config_path)?;

    // Validate that all expressions can be evaluated against default metrics (only if any are defined)
    if !config.deny_if_any.is_empty() || !config.accept_if_any.is_empty() || !config.accept_if_all.is_empty() {
        let metrics: Vec<_> = default_metrics().collect();

        let _ = evaluate(
            &config.deny_if_any,
            &config.accept_if_any,
            &config.accept_if_all,
            &metrics,
            Local::now(),
        )
        .into_app_err("evaluating configuration expressions")?;
    }

    Ok(())
}

pub fn validate_config<H: Host>(host: &mut H, args: &ValidateArgs) -> Result<()> {
    let workspace_root = Utf8PathBuf::from(".");
    let config_path = args.config.as_ref();

    match validate_config_inner(&workspace_root, config_path) {
        Ok(()) => {
            let _ = writeln!(host.output(), "Configuration file is valid");
            if let Some(path) = config_path {
                let _ = writeln!(host.output(), "Config file: {path}");
            } else {
                let _ = writeln!(host.output(), "Using default configuration (no config file found)");
            }
            Ok(())
        }
        Err(e) => {
            let _ = writeln!(host.error(), "❌ Configuration validation failed: {e}");
            host.exit(1);
            Err(e)
        }
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::commands::init::{InitArgs, init_config};
    use std::io::Cursor;

    /// Test host that captures output to in-memory buffers
    struct TestHost {
        output_buf: Vec<u8>,
        error_buf: Vec<u8>,
    }

    impl TestHost {
        fn new() -> Self {
            Self {
                output_buf: Vec::new(),
                error_buf: Vec::new(),
            }
        }
    }

    impl Host for TestHost {
        fn output(&mut self) -> impl Write {
            Cursor::new(&mut self.output_buf)
        }

        fn error(&mut self) -> impl Write {
            Cursor::new(&mut self.error_buf)
        }

        fn exit(&mut self, _code: i32) {
            // In tests, don't actually exit
        }
    }

    #[test]
    fn test_default_config_is_valid() {
        // Create a temporary file path for the test
        let temp_dir = std::env::temp_dir();
        let config_path =
            Utf8PathBuf::from(temp_dir.to_string_lossy().to_string()).join(format!("test_config_{}.toml", std::process::id()));

        // Generate default configuration using init_config
        let mut init_host = TestHost::new();
        let init_args = InitArgs {
            output: config_path.clone(),
        };
        init_config(&mut init_host, &init_args).expect("init_config should succeed");

        // Validate the generated configuration
        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        // Clean up the file
        if let Some(ref path) = args.config {
            let _ = std::fs::remove_file(path);
        }

        assert!(result.is_ok(), "Default configuration should validate successfully: {result:?}");
    }

    #[test]
    fn test_default_config_matches_embedded() {
        // Verify that Config::default() produces the same config as parsing DEFAULT_CONFIG_TOML
        let default_config = Config::default();
        let parsed_config: Config =
            toml::from_str(super::super::config::DEFAULT_CONFIG_TOML).expect("DEFAULT_CONFIG_TOML should parse successfully");

        // Compare the serialized forms to ensure they're equivalent
        let default_toml = toml::to_string(&default_config).expect("default config should serialize");
        let parsed_toml = toml::to_string(&parsed_config).expect("parsed config should serialize");

        assert_eq!(
            default_toml, parsed_toml,
            "Config::default() should match parsing DEFAULT_CONFIG_TOML"
        );
    }

    #[test]
    fn test_invalid_toml_syntax() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("invalid_syntax.toml");

        std::fs::write(
            &config_path,
            r#"
# Missing closing bracket
[[deny_if_any]
name = "test"
expression = "true"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Invalid TOML syntax should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    fn test_unknown_field() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("unknown_field.toml");

        std::fs::write(
            &config_path,
            r#"
deny_if_any = []
accept_if_any = []
accept_if_all = []
unknown_field = "value"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Unknown field should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    fn test_invalid_expression_syntax() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("invalid_expression.toml");

        std::fs::write(
            &config_path,
            r#"
[[deny_if_any]]
name = "invalid_syntax"
description = "Invalid CEL syntax"
expression = "this is not a valid CEL expression !!!"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Invalid expression syntax should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    fn test_invalid_duration_format() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("invalid_duration.toml");

        std::fs::write(
            &config_path,
            r#"
crates_cache_ttl = "not a valid duration"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Invalid duration format should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    fn test_expression_with_nonexistent_metric() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("nonexistent_metric.toml");

        std::fs::write(
            &config_path,
            r#"
[[accept_if_all]]
name = "nonexistent_metric"
description = "Reference to nonexistent metric"
expression = "this_metric_does_not_exist > 100"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Expression referencing nonexistent metric should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    fn test_expression_with_type_mismatch() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("type_mismatch.toml");

        std::fs::write(
            &config_path,
            r#"
[[deny_if_any]]
name = "type_mismatch"
description = "Type mismatch in expression"
expression = "crates.downloads + 'string'"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Expression with type mismatch should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    fn test_expression_returning_non_boolean() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("non_boolean.toml");

        std::fs::write(
            &config_path,
            r#"
[[accept_if_all]]
name = "non_boolean"
description = "Expression returns integer not boolean"
expression = "crates.downloads"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Expression returning non-boolean should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    fn test_empty_config_is_valid() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("empty.toml");

        std::fs::write(&config_path, "# Empty config file\n").expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        assert!(result.is_ok(), "Empty config should be valid (uses defaults)");
    }

    #[test]
    fn test_config_with_only_ttls() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("only_ttls.toml");

        std::fs::write(
            &config_path,
            r#"
crates_cache_ttl = "1h"
hosting_cache_ttl = "2h"
codebase_cache_ttl = "3h"
coverage_cache_ttl = "4h"
advisories_cache_ttl = "5h"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs { config: Some(config_path) };
        let result = validate_config(&mut host, &args);

        assert!(
            result.is_ok(),
            "Config with only TTLs should be valid (expressions default to empty)"
        );
    }
}
