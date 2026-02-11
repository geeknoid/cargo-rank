//! Integration test for config expressions

use cargo_aprz_lib::commands::Config;

#[test]
fn test_load_config_with_expressions() {
    let toml = r#"
[[high_risk_if_any]]
name = "critical_vulnerabilities"
description = "Flag crates with critical vulnerabilities as high risk"
expression = "current_critical_severity_vulnerabilities > 0"

[[high_risk_if_any]]
name = "too_many_dependencies"
expression = "transitive_dependencies > 200"

[[eval]]
name = "good_coverage"
description = "Must have good test coverage"
expression = "code_coverage_percentage >= 70.0"

[[eval]]
name = "no_current_vulnerabilities"
expression = "current_vulnerabilities == 0"
"#;

    let config: Config = toml::from_str(toml).expect("Could not parse config");

    assert_eq!(config.high_risk_if_any.len(), 2);
    assert_eq!(config.eval.len(), 2);

    // Check high_risk_if_any
    assert_eq!(config.high_risk_if_any[0].name(), "critical_vulnerabilities");
    assert_eq!(
        config.high_risk_if_any[0].description(),
        Some("Flag crates with critical vulnerabilities as high risk")
    );
    assert_eq!(
        config.high_risk_if_any[0].expression(),
        "current_critical_severity_vulnerabilities > 0"
    );

    assert_eq!(config.high_risk_if_any[1].name(), "too_many_dependencies");
    assert_eq!(config.high_risk_if_any[1].description(), None);
    assert_eq!(config.high_risk_if_any[1].expression(), "transitive_dependencies > 200");

    // Check eval
    assert_eq!(config.eval[0].name(), "good_coverage");
    assert_eq!(config.eval[0].expression(), "code_coverage_percentage >= 70.0");

    assert_eq!(config.eval[1].name(), "no_current_vulnerabilities");
    assert_eq!(config.eval[1].expression(), "current_vulnerabilities == 0");
}

#[test]
fn test_config_with_empty_expressions() {
    let toml = "
high_risk_if_any = []
eval = []
";

    let config: Config = toml::from_str(toml).expect("Could not parse config");

    assert_eq!(config.high_risk_if_any.len(), 0);
    assert_eq!(config.eval.len(), 0);
}

#[test]
fn test_config_without_expressions() {
    let toml = "
# No expression fields specified
";

    let config: Config = toml::from_str(toml).expect("Could not parse config");

    // Should default to empty vectors
    assert_eq!(config.high_risk_if_any.len(), 0);
    assert_eq!(config.eval.len(), 0);
}

#[test]
fn test_config_with_invalid_expression() {
    let toml = r#"
[[high_risk_if_any]]
name = "bad_expr"
expression = "(unclosed parenthesis"
"#;

    let result: Result<Config, _> = toml::from_str(toml);
    assert!(result.is_err(), "Should fail to parse config with invalid expression");
}
