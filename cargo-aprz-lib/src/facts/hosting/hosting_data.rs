use super::age_stats::AgeStats;
use super::time_window_stats::TimeWindowStats;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostingData {
    pub stars: u64,
    pub forks: u64,
    pub subscribers: u64,

    // Issues

    pub open_issues: u64, 
    pub open_issue_age: AgeStats,
    pub issues_opened: TimeWindowStats,
    pub issues_closed: TimeWindowStats,
    pub closed_issue_age: AgeStats,
    pub closed_issue_age_last_90_days: AgeStats,
    pub closed_issue_age_last_180_days: AgeStats,
    pub closed_issue_age_last_365_days: AgeStats,

    // Pull Requests
    
    pub open_prs: u64,
    pub open_pr_age: AgeStats,
    pub prs_opened: TimeWindowStats,
    pub prs_merged: TimeWindowStats,
    pub prs_closed: TimeWindowStats,
    pub merged_pr_age: AgeStats,
    pub merged_pr_age_last_90_days: AgeStats,
    pub merged_pr_age_last_180_days: AgeStats,
    pub merged_pr_age_last_365_days: AgeStats,
}
