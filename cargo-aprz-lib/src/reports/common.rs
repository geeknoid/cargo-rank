//! Common utilities shared across report generators.

use crate::expr::Risk;
use crate::metrics::{Metric, MetricCategory, MetricValue};
use std::collections::{HashMap, HashSet};

/// Format a metric value as a string using consistent formatting rules.
///
/// `DateTime` values are formatted as date-only (YYYY-MM-DD) for readability.
/// `List` values are formatted as comma-separated strings.
pub fn format_metric_value(value: &MetricValue) -> String {
    match value {
        MetricValue::UInt(u) => u.to_string(),
        MetricValue::Float(f) => format!("{f:.2}"),
        MetricValue::Boolean(b) => b.to_string(),
        MetricValue::String(s) => s.to_string(),
        MetricValue::DateTime(dt) => dt.format("%Y-%m-%d").to_string(),
        MetricValue::List(values) => {
            let mut result = String::new();
            for (i, value) in values.iter().enumerate() {
                if i > 0 {
                    result.push_str(", ");
                }
                result.push_str(&format_metric_value(value));
            }
            result
        }
    }
}

/// Check if a string is a URL (starts with http:// or https://).
pub fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Check if a metric name represents keywords.
pub fn is_keywords_metric(metric_name: &str) -> bool {
    metric_name.to_lowercase().contains("keyword")
}

/// Check if a metric name represents categories.
pub fn is_categories_metric(metric_name: &str) -> bool {
    metric_name.to_lowercase().contains("categor")
}

/// Format keywords or categories with # prefix for each item.
///
/// Takes a comma-separated string and returns a formatted string with # prefix for each item.
/// Example: "rust, cli, tool" becomes "#rust, #cli, #tool"
/// Returns an empty string if the input is empty.
pub fn format_keywords_or_categories_with_prefix(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }
    let items: Vec<String> = value.split(',').map(|item| format!("#{}", item.trim())).collect();
    items.join(", ")
}

/// Format a risk level as a consistent string.
pub const fn format_risk_status(risk: Risk) -> &'static str {
    match risk {
        Risk::Low => "LOW RISK",
        Risk::Medium => "MEDIUM RISK",
        Risk::High => "HIGH RISK",
    }
}

/// Group metrics by category.
///
/// Returns a `HashMap` mapping each category to a vector of metric names.
pub fn group_metrics_by_category<'a>(metrics: &'a [Metric]) -> HashMap<MetricCategory, Vec<&'a str>> {
    let mut metrics_by_category: HashMap<MetricCategory, Vec<&'a str>> = HashMap::new();

    for metric in metrics {
        metrics_by_category.entry(metric.category()).or_default().push(metric.name());
    }

    metrics_by_category
}

/// Group metrics by category across multiple crates, producing the union of all metric names.
///
/// Each metric name appears at most once per category, in the order first encountered.
pub fn group_all_metrics_by_category(crate_metrics: &[&[Metric]]) -> HashMap<MetricCategory, Vec<String>> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut metrics_by_category: HashMap<MetricCategory, Vec<String>> = HashMap::new();

    for &metrics in crate_metrics {
        for metric in metrics {
            if seen.insert(metric.name().to_string()) {
                metrics_by_category
                    .entry(metric.category())
                    .or_default()
                    .push(metric.name().to_string());
            }
        }
    }

    metrics_by_category
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metrics::MetricDef;
    use chrono::{DateTime, Utc};

    static METRIC1_DEF: MetricDef = MetricDef {
        name: "metric1",
        description: "desc1",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    static METRIC2_DEF: MetricDef = MetricDef {
        name: "metric2",
        description: "desc2",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    static METADATA_METRIC_DEF: MetricDef = MetricDef {
        name: "metadata_metric",
        description: "desc",
        category: MetricCategory::Metadata,
        extractor: |_| None,
        default_value: || None,
    };

    static STABILITY_METRIC_DEF: MetricDef = MetricDef {
        name: "stability_metric",
        description: "desc",
        category: MetricCategory::Stability,
        extractor: |_| None,
        default_value: || None,
    };

    #[test]
    fn test_format_metric_value_unsigned_integer() {
        assert_eq!(format_metric_value(&MetricValue::UInt(100)), "100");
        assert_eq!(format_metric_value(&MetricValue::UInt(0)), "0");
    }

    #[test]
    fn test_format_metric_value_float() {
        assert_eq!(format_metric_value(&MetricValue::Float(1.2345)), "1.23");
        assert_eq!(format_metric_value(&MetricValue::Float(0.0)), "0.00");
        assert_eq!(format_metric_value(&MetricValue::Float(99.999)), "100.00");
    }

    #[test]
    fn test_format_metric_value_boolean() {
        assert_eq!(format_metric_value(&MetricValue::Boolean(true)), "true");
        assert_eq!(format_metric_value(&MetricValue::Boolean(false)), "false");
    }

    #[test]
    fn test_format_metric_value_text() {
        assert_eq!(format_metric_value(&MetricValue::String("hello".into())), "hello");
        assert_eq!(format_metric_value(&MetricValue::String("".into())), "");
    }

    #[test]
    fn test_format_metric_value_datetime() {
        let dt = DateTime::parse_from_rfc3339("2024-01-15T10:30:00Z").unwrap();
        let dt_utc: DateTime<Utc> = dt.into();

        // All datetime values show only the date
        let formatted = format_metric_value(&MetricValue::DateTime(dt_utc));
        assert_eq!(formatted, "2024-01-15");
    }

    #[test]
    fn test_is_url() {
        assert!(is_url("http://example.com"));
        assert!(is_url("https://example.com"));
        assert!(is_url("https://github.com/user/repo"));
        assert!(!is_url("example.com"));
        assert!(!is_url("ftp://example.com"));
        assert!(!is_url(""));
    }

    #[test]
    fn test_is_keywords_metric() {
        assert!(is_keywords_metric("keywords"));
        assert!(is_keywords_metric("Keywords"));
        assert!(is_keywords_metric("KEYWORDS"));
        assert!(is_keywords_metric("crate_keywords"));
        assert!(!is_keywords_metric("keys"));
        assert!(!is_keywords_metric(""));
    }

    #[test]
    fn test_is_categories_metric() {
        assert!(is_categories_metric("categories"));
        assert!(is_categories_metric("Categories"));
        assert!(is_categories_metric("CATEGORIES"));
        assert!(is_categories_metric("crate_categories"));
        assert!(is_categories_metric("category"));
        assert!(!is_categories_metric("cats"));
        assert!(!is_categories_metric(""));
    }

    #[test]
    fn test_format_keywords_or_categories_with_prefix() {
        assert_eq!(format_keywords_or_categories_with_prefix("rust"), "#rust");
        assert_eq!(format_keywords_or_categories_with_prefix("rust, cli, tool"), "#rust, #cli, #tool");
        assert_eq!(format_keywords_or_categories_with_prefix("rust,cli,tool"), "#rust, #cli, #tool");
        assert_eq!(format_keywords_or_categories_with_prefix("  rust  ,  cli  "), "#rust, #cli");
    }

    #[test]
    fn test_format_keywords_or_categories_with_prefix_empty_input() {
        assert_eq!(format_keywords_or_categories_with_prefix(""), "");
    }

    #[test]
    fn test_format_risk_status() {
        assert_eq!(format_risk_status(Risk::Low), "LOW RISK");
        assert_eq!(format_risk_status(Risk::Medium), "MEDIUM RISK");
        assert_eq!(format_risk_status(Risk::High), "HIGH RISK");
    }

    #[test]
    fn test_group_metrics_by_category_empty() {
        let metrics: Vec<Metric> = vec![];
        let grouped = group_metrics_by_category(&metrics);
        assert!(grouped.is_empty());
    }

    #[test]
    fn test_group_metrics_by_category_single_category() {
        let metrics = vec![
            Metric::with_value(&METRIC1_DEF, MetricValue::UInt(1)),
            Metric::with_value(&METRIC2_DEF, MetricValue::UInt(2)),
        ];
        let grouped = group_metrics_by_category(&metrics);
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[&MetricCategory::Metadata].len(), 2);
    }

    #[test]
    fn test_group_metrics_by_category_multiple_categories() {
        let metrics = vec![
            Metric::with_value(&METADATA_METRIC_DEF, MetricValue::UInt(1)),
            Metric::with_value(&STABILITY_METRIC_DEF, MetricValue::UInt(2)),
        ];
        let grouped = group_metrics_by_category(&metrics);
        assert_eq!(grouped.len(), 2);
        assert!(grouped.contains_key(&MetricCategory::Metadata));
        assert!(grouped.contains_key(&MetricCategory::Stability));
    }
}
