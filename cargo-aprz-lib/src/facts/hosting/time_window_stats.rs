use serde::{Deserialize, Serialize};

/// Counts of events over different time windows.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimeWindowStats {
    pub last_90_days: u64,
    pub last_180_days: u64,
    pub last_365_days: u64,
    pub total: u64,
}
