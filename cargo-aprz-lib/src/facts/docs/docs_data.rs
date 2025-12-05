use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocsData {
    pub timestamp: DateTime<Utc>,
    pub metrics: DocMetricState,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum DocMetricState {
    Found(DocsMetrics),
    UnknownFormatVersion(u64),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DocsMetrics {
    pub doc_coverage_percentage: f64,
    pub public_api_elements: u64,
    pub undocumented_elements: u64,
    pub examples_in_docs: u64,
    pub has_crate_level_docs: bool,
    pub broken_doc_links: u64,
}
