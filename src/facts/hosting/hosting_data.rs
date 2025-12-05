use crate::facts::hosting::issue_stats::IssueStats;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostingData {
    pub timestamp: DateTime<Utc>,
    pub stars: u64,
    pub forks: u64,
    pub subscribers: u64,
    pub contributors: u64,
    pub commits_last_3_months: u64,
    pub issues: IssueStats,
    pub pulls: IssueStats,
}
