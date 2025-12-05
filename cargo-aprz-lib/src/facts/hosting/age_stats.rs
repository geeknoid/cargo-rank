use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgeStats {
    pub avg: u32,
    pub p50: u32,
    pub p75: u32,
    pub p90: u32,
    pub p95: u32,
}
