use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodebaseData {
    pub timestamp: DateTime<Utc>,
    pub source_files_analyzed: u64,
    pub source_files_with_errors: u64,
    pub production_lines: u64,
    pub test_lines: u64,
    pub comment_lines: u64,
    pub unsafe_count: u64,
    pub example_count: u64,
    pub transitive_dependencies: u64,
    pub workflows_detected: bool,
    pub miri_detected: bool,
    pub clippy_detected: bool,
    pub contributors: u64,
    pub commits_last_90_days: u64,
    pub commits_last_180_days: u64,
    pub commits_last_365_days: u64,
    pub commit_count: u64,
    pub last_commit_at: DateTime<Utc>,
}
