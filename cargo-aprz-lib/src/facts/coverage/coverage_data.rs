use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CoverageData {
    pub code_coverage_percentage: f64,
}
