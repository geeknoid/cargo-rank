//! Integration test for config expressions

use cargo_aprz_lib::commands::Config;

#[test]
fn test_load_config_with_expressions() {
    let toml = r#"
[[deny_if_any]]
name = "critical_vulnerabilities"
description = "Deny crates with critical vulnerabilities"
expression = "current_critical_severity_vulnerabilities > 0"

[[deny_if_any]]
name = "too_many_dependencies"
expression = "transitive_dependencies > 200"

[[accept_if_any]]
name = "very_popular"
description = "Auto-accept extremely popular crates"
expression = "repository_stars > 5000"

[[accept_if_all]]
name = "good_coverage"
description = "Must have good test coverage"
expression = "code_coverage_percentage >= 70.0"

[[accept_if_all]]
name = "no_current_vulnerabilities"
expression = "current_vulnerabilities == 0"
"#;

    let config: Config = toml::from_str(toml).expect("Could not parse config");

    assert_eq!(config.deny_if_any.len(), 2);
    assert_eq!(config.accept_if_any.len(), 1);
    assert_eq!(config.accept_if_all.len(), 2);

    // Check deny_if_any
    assert_eq!(config.deny_if_any[0].name(), "critical_vulnerabilities");
    assert_eq!(
        config.deny_if_any[0].description(),
        Some("Deny crates with critical vulnerabilities")
    );
    assert_eq!(config.deny_if_any[0].expression(), "current_critical_severity_vulnerabilities > 0");

    assert_eq!(config.deny_if_any[1].name(), "too_many_dependencies");
    assert_eq!(config.deny_if_any[1].description(), None);
    assert_eq!(config.deny_if_any[1].expression(), "transitive_dependencies > 200");

    // Check accept_if_any
    assert_eq!(config.accept_if_any[0].name(), "very_popular");
    assert_eq!(config.accept_if_any[0].expression(), "repository_stars > 5000");

    // Check accept_if_all
    assert_eq!(config.accept_if_all[0].name(), "good_coverage");
    assert_eq!(config.accept_if_all[0].expression(), "code_coverage_percentage >= 70.0");

    assert_eq!(config.accept_if_all[1].name(), "no_current_vulnerabilities");
    assert_eq!(config.accept_if_all[1].expression(), "current_vulnerabilities == 0");
}

#[test]
fn test_config_with_empty_expressions() {
    let toml = "
deny_if_any = []
accept_if_any = []
accept_if_all = []
";

    let config: Config = toml::from_str(toml).expect("Could not parse config");

    assert_eq!(config.deny_if_any.len(), 0);
    assert_eq!(config.accept_if_any.len(), 0);
    assert_eq!(config.accept_if_all.len(), 0);
}

#[test]
fn test_config_without_expressions() {
    let toml = "
# No expression fields specified
";

    let config: Config = toml::from_str(toml).expect("Could not parse config");

    // Should default to empty vectors
    assert_eq!(config.deny_if_any.len(), 0);
    assert_eq!(config.accept_if_any.len(), 0);
    assert_eq!(config.accept_if_all.len(), 0);
}

#[test]
fn test_config_with_invalid_expression() {
    let toml = r#"
[[deny_if_any]]
name = "bad_expr"
expression = "(unclosed parenthesis"
"#;

    let result: Result<Config, _> = toml::from_str(toml);
    assert!(result.is_err(), "Should fail to parse config with invalid expression");
}
