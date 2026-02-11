use super::{ReportableCrate, common};
use crate::Result;
use crate::metrics::MetricValue;
use core::fmt::Write;
use serde_json::json;

#[expect(unused_results, reason = "HashMap::insert intentionally overwrites values")]
pub fn generate<W: Write>(crates: &[ReportableCrate], writer: &mut W) -> Result<()> {
    let mut crate_data = Vec::with_capacity(crates.len());

    for crate_info in crates {
        let mut crate_obj = serde_json::Map::new();
        crate_obj.insert("name".to_string(), json!(crate_info.name));
        crate_obj.insert("version".to_string(), json!(crate_info.version.to_string()));

        if let Some(appraisal) = &crate_info.appraisal {
            let mut eval_obj = serde_json::Map::new();
            eval_obj.insert("result".to_string(), json!(common::format_risk_status(appraisal.risk)));
            eval_obj.insert("reasons".to_string(), json!(appraisal.expression_outcomes.iter().map(|o| {
                if o.result {
                    format!("✔️{}", o.name)
                } else {
                    format!("🗙{}", o.name)
                }
            }).collect::<Vec<_>>()));
            crate_obj.insert("appraisal".to_string(), json!(eval_obj));
        }

        let mut metrics_obj = serde_json::Map::new();
        for metric in &crate_info.metrics {
            if let Some(ref value) = metric.value {
                let json_value = metric_value_to_json(value);
                metrics_obj.insert(metric.name().to_string(), json_value);
            }
        }

        crate_obj.insert("metrics".to_string(), json!(metrics_obj));
        crate_data.push(json!(crate_obj));
    }

    let output = json!({
        "crates": crate_data
    });

    write!(writer, "{}", serde_json::to_string_pretty(&output)?)?;
    Ok(())
}

fn metric_value_to_json(value: &MetricValue) -> serde_json::Value {
    match value {
        MetricValue::UInt(u) => json!(u),
        MetricValue::Float(f) => json!(f),
        MetricValue::Boolean(b) => json!(b),
        MetricValue::String(s) => json!(s.as_str()),
        MetricValue::DateTime(dt) => json!(dt.format("%Y-%m-%d").to_string()),
        MetricValue::List(values) => {
            // Convert list to JSON array, recursively converting each element
            json!(values.iter().map(metric_value_to_json).collect::<Vec<_>>())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::{Appraisal, ExpressionOutcome, Risk};
    use crate::metrics::{Metric, MetricCategory, MetricDef};
    use chrono::{DateTime, Utc};
    use std::sync::Arc;

    static NAME_DEF: MetricDef = MetricDef {
        name: "name",
        description: "Crate name",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    static VERSION_DEF: MetricDef = MetricDef {
        name: "version",
        description: "Crate version",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    fn create_test_crate(name: &str, version: &str, evaluation: Option<Appraisal>) -> ReportableCrate {
        let metrics = vec![
            Metric::with_value(&NAME_DEF, MetricValue::String(name.into())),
            Metric::with_value(&VERSION_DEF, MetricValue::String(version.into())),
        ];
        ReportableCrate::new(name.into(), Arc::new(version.parse().unwrap()), metrics, evaluation)
    }

    #[test]
    fn test_metric_value_to_json_float() {
        let value = MetricValue::Float(1.234);
        let json = metric_value_to_json(&value);
        assert_eq!(json, json!(1.234));
    }

    #[test]
    fn test_metric_value_to_json_boolean() {
        let value = MetricValue::Boolean(true);
        let json = metric_value_to_json(&value);
        assert_eq!(json, json!(true));
    }

    #[test]
    fn test_metric_value_to_json_text() {
        let value = MetricValue::String("hello".into());
        let json = metric_value_to_json(&value);
        assert_eq!(json, json!("hello"));
    }

    #[test]
    fn test_metric_value_to_json_datetime() {
        let dt = DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z").unwrap();
        let dt_utc: DateTime<Utc> = dt.into();
        let value = MetricValue::DateTime(dt_utc);
        let json = metric_value_to_json(&value);
        assert!(json.as_str().unwrap().contains("2024-01-15"));
    }

    #[test]
    fn test_generate_empty_crates() {
        let crates: Vec<ReportableCrate> = vec![];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert!(parsed["crates"].is_array());
        assert_eq!(parsed["crates"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_generate_single_crate_no_evaluation() {
        let crates = vec![create_test_crate("test_crate", "1.2.3", None)];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["crates"][0]["name"], "test_crate");
        assert_eq!(parsed["crates"][0]["version"], "1.2.3");
        // Should not have evaluation
        assert!(parsed["crates"][0]["evaluation"].is_null());
    }

    #[test]
    fn test_generate_single_crate_with_evaluation() {
        let eval = Appraisal {
            risk: Risk::Low,
            expression_outcomes: vec![ExpressionOutcome::new("good".into(), "Good".into(), true)],
        };
        let crates = vec![create_test_crate("test_crate", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["crates"][0]["appraisal"]["result"], "LOW RISK");
        assert_eq!(parsed["crates"][0]["appraisal"]["reasons"][0], "✔\u{fe0f}good");
    }

    #[test]
    fn test_generate_multiple_crates() {
        let crates = vec![
            create_test_crate("crate_a", "1.0.0", None),
            create_test_crate("crate_b", "2.0.0", None),
        ];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["crates"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["crates"][0]["name"], "crate_a");
        assert_eq!(parsed["crates"][1]["name"], "crate_b");
    }

    #[test]
    fn test_generate_denied_status() {
        let eval = Appraisal {
            risk: Risk::High,
            expression_outcomes: vec![ExpressionOutcome::new("security".into(), "Security issue".into(), false)],
        };
        let crates = vec![create_test_crate("bad_crate", "1.0.0", Some(eval))];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed["crates"][0]["appraisal"]["result"], "HIGH RISK");
    }

    #[test]
    fn test_generate_pretty_formatting() {
        let crates = vec![create_test_crate("test", "1.0.0", None)];
        let mut output = String::new();
        let result = generate(&crates, &mut output);
        result.unwrap();
        // Pretty-printed JSON should have newlines and indentation
        assert!(output.contains('\n'));
        assert!(output.contains("  "));
    }
}
