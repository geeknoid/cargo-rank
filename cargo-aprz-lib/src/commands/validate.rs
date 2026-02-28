use super::Host;
use super::config::Config;
use crate::Result;
use crate::expr::{ExpressionDisposition, evaluate};
use crate::metrics::default_metrics;
use camino::{Utf8Path, Utf8PathBuf};
use cargo_metadata::MetadataCommand;
use chrono::Local;
use clap::Parser;
use ohno::{IntoAppError, app_err};
use std::io::Write;

#[derive(Parser, Debug)]
pub struct ValidateArgs {
    /// Path to configuration file (defaults to `aprz.toml` in workspace root)
    #[arg(value_name = "PATH")]
    pub config: Option<Utf8PathBuf>,

    /// Path to Cargo.toml file
    #[arg(long, default_value = "Cargo.toml", value_name = "PATH")]
    pub manifest_path: Utf8PathBuf,
}

pub fn validate_config<H: Host>(host: &mut H, args: &ValidateArgs) -> Result<()> {
    let config_path = if let Some(path) = &args.config {
        path.clone()
    } else {
        let mut metadata_cmd = MetadataCommand::new();
        let _ = metadata_cmd.manifest_path(&args.manifest_path);
        let metadata = metadata_cmd.exec().into_app_err("retrieving workspace metadata")?;
        metadata.workspace_root.join("aprz.toml")
    };

    if !config_path.as_std_path().exists() {
        return Err(app_err!("could not find configuration file '{config_path}'"));
    }

    validate_config_inner(&config_path)?;

    let _ = writeln!(host.output(), "Configuration file at '{config_path}' is valid");
    Ok(())
}

/// Validates a configuration file by loading it and checking expression evaluation
///
/// # Errors
///
/// Returns an error if the config file cannot be loaded, parsed, or if expressions fail to evaluate
fn validate_config_inner(config_path: &Utf8Path) -> Result<()> {
    let config = Config::load(config_path.parent().unwrap_or_else(|| Utf8Path::new(".")), Some(&config_path.to_path_buf()))?;

    // Validate that all expressions can be evaluated against default metrics (only if any are defined)
    if !config.high_risk.is_empty() || !config.eval.is_empty() {
        let appraisal = evaluate(
            &config.high_risk,
            &config.eval,
            default_metrics(),
            Local::now(),
            config.medium_risk_threshold,
            config.low_risk_threshold,
        );

        // Check if any expression evaluation failed
        for outcome in &appraisal.expression_outcomes {
            if let ExpressionDisposition::Failed(msg) = &outcome.disposition {
                return Err(app_err!("expression '{}' failed: {msg}", outcome.name));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use crate::commands::init::{InitArgs, init_config};
    use crate::commands::host::TestHost;

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_default_config_is_valid() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("test_config.toml");

        // Generate default configuration using init_config
        let mut init_host = TestHost::new();
        let init_args = InitArgs {
            output: Some(config_path.clone()),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        init_config(&mut init_host, &init_args).expect("init_config should succeed");

        // Validate the generated configuration
        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

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
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_invalid_toml_syntax() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("invalid_syntax.toml");

        std::fs::write(
            &config_path,
            r#"
# Missing closing bracket
[[high_risk]
name = "test"
expression = "true"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Invalid TOML syntax should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_unknown_field() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("unknown_field.toml");

        std::fs::write(
            &config_path,
            r#"
high_risk = []
eval = []
unknown_field = "value"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Unknown field should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_invalid_expression_syntax() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("invalid_expression.toml");

        std::fs::write(
            &config_path,
            r#"
[[high_risk]]
name = "invalid_syntax"
description = "Invalid CEL syntax"
expression = "this is not a valid CEL expression !!!"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Invalid expression syntax should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
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
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Invalid duration format should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_expression_with_nonexistent_metric() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("nonexistent_metric.toml");

        std::fs::write(
            &config_path,
            r#"
[[eval]]
name = "nonexistent_metric"
description = "Reference to nonexistent metric"
expression = "this_metric_does_not_exist > 100"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Expression referencing nonexistent metric should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_expression_with_type_mismatch() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("type_mismatch.toml");

        std::fs::write(
            &config_path,
            r#"
[[high_risk]]
name = "type_mismatch"
description = "Type mismatch in expression"
expression = "crates.downloads + 'string'"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Expression with type mismatch should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_expression_returning_non_boolean() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("non_boolean.toml");

        std::fs::write(
            &config_path,
            r#"
[[eval]]
name = "non_boolean"
description = "Expression returns integer not boolean"
expression = "crates.downloads"
"#,
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err(), "Expression returning non-boolean should fail validation");

        // Extract just the error message before the context chain and backtrace
        let error_msg = result.unwrap_err().to_string();
        let without_context = error_msg.split("\n>").next().unwrap_or(&error_msg);
        let snapshot_content = without_context.split("\nBacktrace:").next().unwrap_or(without_context).trim();
        insta::assert_snapshot!(snapshot_content);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_empty_config_is_valid() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("empty.toml");

        std::fs::write(&config_path, "# Empty config file\n").expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_ok(), "Empty config should be valid (uses defaults)");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
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
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(
            result.is_ok(),
            "Config with only TTLs should be valid (expressions default to empty)"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_medium_risk_threshold_below_range() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("bad_threshold.toml");

        std::fs::write(&config_path, "medium_risk_threshold = -1.0\n").expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("medium_risk_threshold must be between 0 and 100"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_medium_risk_threshold_above_range() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("bad_threshold.toml");

        std::fs::write(&config_path, "medium_risk_threshold = 101.0\n").expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("medium_risk_threshold must be between 0 and 100"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_low_risk_threshold_below_range() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("bad_threshold.toml");

        std::fs::write(&config_path, "low_risk_threshold = -5.0\n").expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("low_risk_threshold must be between 0 and 100"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_low_risk_threshold_above_range() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("bad_threshold.toml");

        std::fs::write(&config_path, "low_risk_threshold = 200.0\n").expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("low_risk_threshold must be between 0 and 100"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_medium_threshold_not_less_than_low_threshold() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("bad_threshold.toml");

        std::fs::write(
            &config_path,
            "medium_risk_threshold = 80.0\nlow_risk_threshold = 50.0\n",
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("medium_risk_threshold (80) must be less than low_risk_threshold (50)"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_equal_thresholds_rejected() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("bad_threshold.toml");

        std::fs::write(
            &config_path,
            "medium_risk_threshold = 50.0\nlow_risk_threshold = 50.0\n",
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("must be less than"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_valid_custom_thresholds() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let config_path = Utf8PathBuf::from(temp_dir.path().to_string_lossy().to_string()).join("custom_thresholds.toml");

        std::fs::write(
            &config_path,
            "medium_risk_threshold = 25.0\nlow_risk_threshold = 75.0\n",
        )
        .expect("Failed to write test config");

        let mut host = TestHost::new();
        let args = ValidateArgs {
            config: Some(config_path),
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };
        let result = validate_config(&mut host, &args);

        assert!(result.is_ok(), "Valid custom thresholds should pass validation: {result:?}");
    }
}
