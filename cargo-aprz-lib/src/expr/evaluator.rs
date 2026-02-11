//! Evaluator for determining crate acceptance status using expressions
//!
//! This module provides functionality to evaluate crates against configured
//! expressions and determine if they should be ACCEPTED, DENIED, or NOT EVALUATED.

use super::{Appraisal, Expression, ExpressionOutcome, Risk};
use crate::Result;
use crate::metrics::{Metric, MetricValue};
use cel_interpreter::{Context, Program, Value, objects::Map};
use chrono::{DateTime, Local};
use ohno::{IntoAppError, app_err};
use std::sync::Arc;

/// Evaluate expressions against metrics and determine risk level
///
/// # Evaluation order:
///
/// 1. First, check `high_risk_if_any` expressions - if ANY is true, crate is HIGH RISK
/// 2. Then, evaluate ALL `eval` expressions, summing granted vs possible points
/// 3. Compute score = granted / possible * 100, compare against thresholds
/// 4. If no expressions defined, returns LOW RISK with score 100
///
/// Expressions without explicit points default to 1 point each.
///
/// # Returns
///
/// `Ok(EvaluationOutcome)` with the risk level, score, and reasons, or `Err(AppError)` if evaluation fails
///
/// # Errors
///
/// Returns an error if expression evaluation fails or returns a non-boolean value
pub fn evaluate(
    high_risk_if_any: &[Expression],
    eval: &[Expression],
    metrics: impl IntoIterator<Item: core::borrow::Borrow<Metric>>,
    now: DateTime<Local>,
    medium_risk_threshold: f64,
    low_risk_threshold: f64,
) -> Result<Appraisal> {
    let context = build_cel_context(metrics, now);

    for expr in high_risk_if_any {
        match evaluate_expression(expr.program(), expr.name(), &context) {
            Ok(true) => {
                return Ok(Appraisal::new(
                    Risk::High,
                    vec![ExpressionOutcome::new(
                        expr.name_arc(),
                        expr.description_or_expression_arc(),
                        true,
                    )],
                ));
            }
            Ok(false) => {
                // Expression evaluated to false, continue checking
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    if high_risk_if_any.is_empty() && eval.is_empty() {
        return Ok(Appraisal::new(Risk::Low, vec![]));
    }

    let mut total_possible_points: u32 = 0;
    let mut total_granted_points: u32 = 0;
    let mut outcomes = Vec::with_capacity(eval.len());

    for expr in eval {
        let points = expr.points().unwrap_or(1);
        total_possible_points += points;

        match evaluate_expression(expr.program(), expr.name(), &context) {
            Ok(result) => {
                if result {
                    total_granted_points += points;
                }
                outcomes.push(ExpressionOutcome::new(
                    expr.name_arc(),
                    expr.description_or_expression_arc(),
                    result,
                ));
            }
            Err(e) => {
                return Err(e);
            }
        }
    }

    let score = if total_possible_points > 0 {
        f64::from(total_granted_points) / f64::from(total_possible_points) * 100.0
    } else {
        100.0
    };

    let risk = if score >= low_risk_threshold {
        Risk::Low
    } else if score >= medium_risk_threshold {
        Risk::Medium
    } else {
        Risk::High
    };

    Ok(Appraisal::new(risk, outcomes))
}

/// Evaluates a pre-parsed boolean expression against a context
fn evaluate_expression(program: &Program, name: &str, context: &Context) -> Result<bool> {
    match program
        .execute(context)
        .into_app_err(format!("Could not evaluate expression '{name}'"))?
    {
        Value::Bool(b) => Ok(b),
        other => Err(app_err!("Expression '{name}' did not return a boolean, got '{other:?}' instead")),
    }
}

fn build_cel_context(metrics: impl IntoIterator<Item: core::borrow::Borrow<Metric>>, now: DateTime<Local>) -> Context<'static> {
    use core::borrow::Borrow;
    use std::collections::HashMap;

    let mut context = Context::default();

    // Build nested map structure for dotted metric names
    let mut root_map: HashMap<&str, HashMap<Arc<String>, Value>> = HashMap::with_capacity(16);
    let mut flat_vars: Vec<(&str, Value)> = Vec::with_capacity(16);

    for metric in metrics {
        let metric: &Metric = metric.borrow();
        let cel_value = metric.value.as_ref().map_or(Value::Null, convert_metric_value);
        let name = metric.name();

        // Split on first dot only
        if let Some((prefix, suffix)) = name.split_once('.') {
            let _ = root_map
                .entry(prefix)
                .or_default()
                .insert(Arc::new(suffix.to_string()), cel_value);
        } else {
            // No dot, add as flat variable
            flat_vars.push((name, cel_value));
        }
    }

    // Add nested structures to context
    for (prefix, fields) in root_map {
        let cel_map = Map::from(fields);
        context.add_variable_from_value(prefix, Value::Map(cel_map));
    }

    // Add flat variables
    for (name, value) in flat_vars {
        context.add_variable_from_value(name, value);
    }

    // Add now variable
    context.add_variable_from_value("now", Value::Timestamp(now.fixed_offset()));

    context
}

/// Convert a `MetricValue` to a CEL Value
fn convert_metric_value(value: &MetricValue) -> Value {
    match value {
        MetricValue::UInt(u) => Value::UInt(*u),
        MetricValue::Float(f) => Value::Float(*f),
        MetricValue::Boolean(b) => Value::Bool(*b),
        MetricValue::String(s) => Value::String(Arc::new(s.to_string())),
        MetricValue::DateTime(dt) => Value::Timestamp(dt.fixed_offset()),
        MetricValue::List(values) => {
            // Recursively convert each MetricValue to CEL Value
            let cel_values: Vec<Value> = values.iter().map(convert_metric_value).collect();
            Value::List(Arc::new(cel_values))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const MEDIUM_THRESHOLD: f64 = 30.0;
    const LOW_THRESHOLD: f64 = 70.0;

    fn test_timestamp() -> DateTime<Local> {
        Local.with_ymd_and_hms(2024, 1, 15, 10, 30, 0).unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_not_evaluated_when_no_expressions() {
        let high_risk_expressions = vec![];
        let eval_expressions = vec![];
        let metrics = vec![];

        let outcome = evaluate(
            &high_risk_expressions,
            &eval_expressions,
            &metrics,
            test_timestamp(),
            MEDIUM_THRESHOLD,
            LOW_THRESHOLD,
        )
        .unwrap();
        assert_eq!(outcome.risk, Risk::Low);
        assert!(outcome.expression_outcomes.is_empty());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_evaluation_outcome_creation() {
        let outcomes = vec![
            ExpressionOutcome::new("r1".into(), "reason 1".into(), true),
            ExpressionOutcome::new("r2".into(), "reason 2".into(), false),
        ];
        let outcome = Appraisal::new(Risk::Low, outcomes);
        assert_eq!(outcome.risk, Risk::Low);
        assert_eq!(outcome.expression_outcomes.len(), 2);

        let denied = Appraisal::new(Risk::High, vec![ExpressionOutcome::new("r".into(), "reason".into(), false)]);
        assert_eq!(denied.risk, Risk::High);
        assert_eq!(denied.expression_outcomes.len(), 1);
    }

    // Tests from expr_evaluator
    use crate::metrics::{MetricCategory, MetricDef};

    static STARS_DEF: MetricDef = MetricDef {
        name: "stars",
        description: "Stars",
        category: MetricCategory::Community,
        extractor: |_| None,
        default_value: || None,
    };

    static COVERAGE_DEF: MetricDef = MetricDef {
        name: "coverage",
        description: "Coverage",
        category: MetricCategory::Trustworthiness,
        extractor: |_| None,
        default_value: || None,
    };

    static MISSING_VALUE_DEF: MetricDef = MetricDef {
        name: "missing_value",
        description: "Missing",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    static CREATED_AT_DEF: MetricDef = MetricDef {
        name: "created_at",
        description: "Creation date",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    fn eval_expr(expr: &str, metrics: &[Metric]) -> Result<bool> {
        let program = Program::compile(expr).map_err(|e| app_err!("Could not compile expression: {e}"))?;
        let context = build_cel_context(metrics, test_timestamp());
        evaluate_expression(&program, "test", &context)
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_basic_comparison() {
        let metrics = vec![
            Metric::with_value(&STARS_DEF, MetricValue::UInt(150)),
            Metric::with_value(&COVERAGE_DEF, MetricValue::Float(85.5)),
        ];

        // Test simple comparisons
        assert!(eval_expr("stars > 100", &metrics).unwrap());
        assert!(eval_expr("coverage >= 80.0", &metrics).unwrap());
        assert!(eval_expr("stars > 100 && coverage >= 80.0", &metrics).unwrap());
        assert!(!eval_expr("stars > 200", &metrics).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_null_handling() {
        let metrics = vec![
            Metric::with_value(&STARS_DEF, MetricValue::UInt(150)),
            Metric::new(&MISSING_VALUE_DEF),
        ];

        // Test null checks with ternary operator
        assert!(eval_expr("stars > 100", &metrics).unwrap());
        assert!(eval_expr("missing_value == null", &metrics).unwrap());
        assert!(eval_expr("missing_value != null ? false : true", &metrics).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_now_variable_available() {
        let metrics = vec![Metric::with_value(&STARS_DEF, MetricValue::UInt(150))];

        // Test that "now" variable is available and is a valid timestamp
        assert!(eval_expr("now != null", &metrics).unwrap());

        // Test that "now" can be compared with itself
        assert!(eval_expr("now == now", &metrics).unwrap());

        // Test that we can use "now" in expressions with metrics
        assert!(eval_expr("stars > 100 && now != null", &metrics).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_now_variable_with_datetime_metric() {
        use chrono::{TimeZone, Utc};

        // Create a datetime metric set to a fixed past date
        let past_date = Utc.with_ymd_and_hms(2023, 1, 1, 0, 0, 0).unwrap();
        let metrics = vec![Metric::with_value(&CREATED_AT_DEF, MetricValue::DateTime(past_date))];

        // Test that "now" is greater than a past date
        assert!(eval_expr("now > created_at", &metrics).unwrap());

        // Test that the past date is less than now
        assert!(eval_expr("created_at < now", &metrics).unwrap());
    }

    // Test dotted metric names (e.g., usage.downloads)
    static USAGE_DOWNLOADS_DEF: MetricDef = MetricDef {
        name: "usage.downloads",
        description: "Download count",
        category: MetricCategory::Community,
        extractor: |_| None,
        default_value: || None,
    };

    static USAGE_RECENT_DEF: MetricDef = MetricDef {
        name: "usage.recent_downloads",
        description: "Recent downloads",
        category: MetricCategory::Community,
        extractor: |_| None,
        default_value: || None,
    };

    static METADATA_NAME_DEF: MetricDef = MetricDef {
        name: "metadata.name",
        description: "Crate name",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_dotted_metric_names() {
        let metrics = vec![
            Metric::with_value(&USAGE_DOWNLOADS_DEF, MetricValue::UInt(1000)),
            Metric::with_value(&USAGE_RECENT_DEF, MetricValue::UInt(500)),
        ];

        // Test accessing dotted names
        assert!(eval_expr("usage.downloads > 900", &metrics).unwrap());
        assert!(eval_expr("usage.recent_downloads == 500", &metrics).unwrap());
        assert!(eval_expr("usage.downloads > usage.recent_downloads", &metrics).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_multiple_dotted_prefixes() {
        let metrics = vec![
            Metric::with_value(&USAGE_DOWNLOADS_DEF, MetricValue::UInt(1000)),
            Metric::with_value(&METADATA_NAME_DEF, MetricValue::String("tokio".into())),
        ];

        // Test accessing different prefixes
        assert!(eval_expr("usage.downloads > 500", &metrics).unwrap());
        assert!(eval_expr("metadata.name == 'tokio'", &metrics).unwrap());
        assert!(eval_expr("usage.downloads > 500 && metadata.name == 'tokio'", &metrics).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_mixed_flat_and_dotted_names() {
        let metrics = vec![
            Metric::with_value(&STARS_DEF, MetricValue::UInt(150)),
            Metric::with_value(&USAGE_DOWNLOADS_DEF, MetricValue::UInt(1000)),
        ];

        // Test mixing flat and dotted names
        assert!(eval_expr("stars > 100", &metrics).unwrap());
        assert!(eval_expr("usage.downloads > 500", &metrics).unwrap());
        assert!(eval_expr("stars > 100 && usage.downloads > 500", &metrics).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_dotted_name_with_null_value() {
        let metrics = vec![
            Metric::new(&USAGE_DOWNLOADS_DEF), // No value set
            Metric::with_value(&USAGE_RECENT_DEF, MetricValue::UInt(500)),
        ];

        // Test null handling with dotted names
        assert!(eval_expr("usage.downloads == null", &metrics).unwrap());
        assert!(eval_expr("usage.recent_downloads != null", &metrics).unwrap());
    }

    // Additional tests for error paths and edge cases

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_high_risk_if_any_expression_evaluation_error() {
        let expr = Expression::new("bad_expr", None, "undefined_var > 100", None).unwrap();
        let metrics = vec![];
        let result = evaluate(&[expr], &[], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD);
        let _ = result.unwrap_err();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_eval_expression_evaluation_error() {
        let expr = Expression::new("bad_expr", None, "undefined.field", None).unwrap();
        let metrics = vec![];
        let result = evaluate(&[], &[expr], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD);
        let _ = result.unwrap_err();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_expression_returns_non_boolean() {
        let metrics = vec![Metric::with_value(&STARS_DEF, MetricValue::UInt(150))];
        let result = eval_expr("stars", &metrics);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("did not return a boolean"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_expression_returns_string() {
        let metrics = vec![Metric::with_value(&METADATA_NAME_DEF, MetricValue::String("tokio".into()))];
        let result = eval_expr("metadata.name", &metrics);
        let _ = result.unwrap_err();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_high_risk_if_any_true_no_description() {
        let expr = Expression::new("high_stars", None, "stars > 100", None).unwrap();
        let metrics = vec![Metric::with_value(&STARS_DEF, MetricValue::UInt(150))];
        let outcome = evaluate(&[expr], &[], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.risk, Risk::High);
        assert!(outcome.expression_outcomes[0].name.contains("high_stars"));
        assert!(outcome.expression_outcomes[0].result);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_eval_false_no_description() {
        // Single eval expression that fails => score 0/1 = 0% => High risk
        let expr = Expression::new("high_stars", None, "stars > 200", None).unwrap();
        let metrics = vec![Metric::with_value(&STARS_DEF, MetricValue::UInt(150))];
        let outcome = evaluate(&[], &[expr], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.risk, Risk::High);
        assert!(!outcome.expression_outcomes[0].result);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_high_risk_false_with_empty_eval() {
        let high_risk_expr = Expression::new("d", None, "stars > 200", None).unwrap();
        let metrics = vec![Metric::with_value(&STARS_DEF, MetricValue::UInt(150))];

        let outcome = evaluate(&[high_risk_expr], &[], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.risk, Risk::Low);
        assert_eq!(outcome.expression_outcomes.len(), 0);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_metric_value_list_conversion() {
        static LIST_DEF: MetricDef = MetricDef {
            name: "tags",
            description: "Tags",
            category: MetricCategory::Metadata,
            extractor: |_| None,
            default_value: || None,
        };

        let list_value = MetricValue::List(vec![MetricValue::String("rust".into()), MetricValue::String("async".into())]);
        let metrics = vec![Metric::with_value(&LIST_DEF, list_value)];

        assert!(eval_expr("tags.size() == 2", &metrics).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_nested_list_conversion() {
        static NESTED_LIST_DEF: MetricDef = MetricDef {
            name: "nested",
            description: "Nested",
            category: MetricCategory::Metadata,
            extractor: |_| None,
            default_value: || None,
        };

        let nested = MetricValue::List(vec![
            MetricValue::List(vec![MetricValue::UInt(1), MetricValue::UInt(2)]),
            MetricValue::List(vec![MetricValue::UInt(3)]),
        ]);
        let metrics = vec![Metric::with_value(&NESTED_LIST_DEF, nested)];

        assert!(eval_expr("nested.size() == 2", &metrics).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_boolean_metric_value() {
        static BOOL_DEF: MetricDef = MetricDef {
            name: "has_tests",
            description: "Has tests",
            category: MetricCategory::Trustworthiness,
            extractor: |_| None,
            default_value: || None,
        };

        let metrics = vec![Metric::with_value(&BOOL_DEF, MetricValue::Boolean(true))];
        assert!(eval_expr("has_tests == true", &metrics).unwrap());
        assert!(eval_expr("has_tests", &metrics).unwrap());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_eval_multiple_all_true() {
        let expr1 = Expression::new("e1", Some("desc1"), "stars > 100", None).unwrap();
        let expr2 = Expression::new("e2", Some("desc2"), "coverage > 50.0", None).unwrap();
        let metrics = vec![
            Metric::with_value(&STARS_DEF, MetricValue::UInt(150)),
            Metric::with_value(&COVERAGE_DEF, MetricValue::Float(85.5)),
        ];

        let outcome = evaluate(&[], &[expr1, expr2], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.risk, Risk::Low);
        assert_eq!(outcome.expression_outcomes.len(), 2);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_eval_first_false() {
        // 1 of 2 expressions pass => score 50% => Medium risk (between 30 and 70)
        let expr1 = Expression::new("e1", None, "stars > 200", None).unwrap();
        let expr2 = Expression::new("e2", None, "coverage > 50.0", None).unwrap();
        let metrics = vec![
            Metric::with_value(&STARS_DEF, MetricValue::UInt(150)),
            Metric::with_value(&COVERAGE_DEF, MetricValue::Float(85.5)),
        ];

        let outcome = evaluate(&[], &[expr1, expr2], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.risk, Risk::Medium);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_eval_middle_false() {
        // 2 of 3 expressions pass => score ~66.7% => Medium risk (between 30 and 70)
        let expr1 = Expression::new("e1", None, "stars > 100", None).unwrap();
        let expr2 = Expression::new("e2", None, "coverage > 90.0", None).unwrap();
        let expr3 = Expression::new("e3", None, "stars < 200", None).unwrap();
        let metrics = vec![
            Metric::with_value(&STARS_DEF, MetricValue::UInt(150)),
            Metric::with_value(&COVERAGE_DEF, MetricValue::Float(85.5)),
        ];

        let outcome = evaluate(&[], &[expr1, expr2, expr3], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.risk, Risk::Medium);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_eval_with_explicit_points() {
        // expr1: 10 points, true; expr2: 5 points, false => 10/15 = 66.7% => Medium
        let expr1 = Expression::new("e1", None, "stars > 100", Some(10)).unwrap();
        let expr2 = Expression::new("e2", None, "stars > 200", Some(5)).unwrap();
        let metrics = vec![Metric::with_value(&STARS_DEF, MetricValue::UInt(150))];

        let outcome = evaluate(&[], &[expr1, expr2], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.risk, Risk::Medium);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_eval_weighted_points_high_risk() {
        // expr1: 1 point, true; expr2: 10 points, false => 1/11 = 9.1% => High (below 30)
        let expr1 = Expression::new("e1", None, "stars > 100", Some(1)).unwrap();
        let expr2 = Expression::new("e2", None, "stars > 200", Some(10)).unwrap();
        let metrics = vec![Metric::with_value(&STARS_DEF, MetricValue::UInt(150))];

        let outcome = evaluate(&[], &[expr1, expr2], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.risk, Risk::High);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_eval_all_false_is_high_risk() {
        // 0 of 2 pass => score 0% => High risk
        let expr1 = Expression::new("e1", None, "stars > 200", None).unwrap();
        let expr2 = Expression::new("e2", None, "coverage > 90.0", None).unwrap();
        let metrics = vec![
            Metric::with_value(&STARS_DEF, MetricValue::UInt(150)),
            Metric::with_value(&COVERAGE_DEF, MetricValue::Float(85.5)),
        ];

        let outcome = evaluate(&[], &[expr1, expr2], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.risk, Risk::High);
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn test_eval_reasons_include_all_expressions() {
        // All expressions evaluated, reasons include both passed and failed
        let expr1 = Expression::new("e1", Some("good"), "stars > 100", None).unwrap();
        let expr2 = Expression::new("e2", Some("bad"), "stars > 200", None).unwrap();
        let metrics = vec![Metric::with_value(&STARS_DEF, MetricValue::UInt(150))];

        let outcome = evaluate(&[], &[expr1, expr2], &metrics, test_timestamp(), MEDIUM_THRESHOLD, LOW_THRESHOLD).unwrap();
        assert_eq!(outcome.expression_outcomes.len(), 2);
        assert_eq!(&*outcome.expression_outcomes[0].name, "e1");
        assert_eq!(&*outcome.expression_outcomes[0].description, "good");
        assert!(outcome.expression_outcomes[0].result);
        assert_eq!(&*outcome.expression_outcomes[1].name, "e2");
        assert_eq!(&*outcome.expression_outcomes[1].description, "bad");
        assert!(!outcome.expression_outcomes[1].result);
    }
}
