//! Metric extraction and normalization from crate facts
//!
//! This module transforms rich `CrateFacts` data structures into a flat,
//! normalized representation suitable for evaluation and reporting. It converts
//! nested data into individual named metrics with typed values.
//!
//! # Implementation Model
//!
//! The core abstraction is the [`Metric`] type, which pairs a metric definition
//! ([`MetricDef`]) with an optional value ([`MetricValue`]). Each metric has:
//! - **Name**: Dot-separated identifier (e.g., `community.repo_stars`)
//! - **Description**: Human-readable explanation
//! - **Category**: Organizational grouping ([`MetricCategory`])
//! - **Value**: Typed data (integer, float, boolean, text, datetime, or null)
//!
//! Metric definitions are statically registered in `metric_def.rs` and include
//! an extractor function that knows how to pull the relevant data from a
//! `CrateFacts` instance. The [`flatten`] function processes a `CrateFacts`
//! through all registered extractors to produce a complete metric set.
//!
//! Metrics are intentionally flat rather than hierarchical to simplify expression
//! evaluation and report generation. The dot-notation naming provides logical
//! grouping while maintaining a simple key-value structure.

mod metric;
mod metric_category;
mod metric_def;
mod metric_value;

pub use metric::{Metric, default_metrics, flatten};
pub use metric_category::MetricCategory;
pub use metric_value::MetricValue;

#[cfg(any(debug_assertions, test))]
pub use metric_def::MetricDef;
