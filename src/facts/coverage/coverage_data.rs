use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CoverageData {
    pub timestamp: DateTime<Utc>,
    pub code_coverage_percentage: f64,
}
