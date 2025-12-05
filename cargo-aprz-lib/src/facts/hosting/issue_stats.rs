use super::AgeStats;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IssueStats {
    pub open_count: u64,
    pub closed_count: u64,
    pub open_age: AgeStats,
    pub closed_age: AgeStats,
}
