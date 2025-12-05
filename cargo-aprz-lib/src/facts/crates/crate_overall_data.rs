use super::owner::Owner;
use chrono::{DateTime, NaiveDate, Utc};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use url::Url;

/// Version-independent crate information.
///
/// This struct contains metadata that applies to the crate as a whole, regardless of
/// which specific version is being queried. All data originates from the crates.io
/// database dump.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CrateOverallData {
    /// When the crate was first published to crates.io.
    ///
    /// **Source**: `crates.csv` from the `crates` table, `created_at` field
    pub created_at: DateTime<Utc>,

    /// When the crate's metadata was last updated on crates.io.
    /// This updates when any version is published or crate metadata changes.
    ///
    /// **Source**: `crates.csv` from the `crates` table, `updated_at` field
    pub updated_at: DateTime<Utc>,

    /// Optional URL to the crate's source codebase repository (typically GitHub).
    ///
    /// **Source**: `crates.csv` from the `crates` table, `repository` field
    pub repository: Option<Url>,

    /// List of crates.io category names this crate belongs to.
    /// Categories help organize crates by domain (e.g., "web-programming", "command-line-utilities").
    ///
    /// **Source**: Multi-table join:
    /// 1. `crates_categories.csv` from the `crates_categories` table (`crate_id` to `category_id` mapping)
    /// 2. `categories.csv` from the `categories` table, `category` field (category names)
    pub categories: Vec<CompactString>,

    /// List of keyword strings associated with this crate.
    /// Keywords are user-defined tags for discovery (e.g., "parser", "cli", "async").
    ///
    /// **Source**: Multi-table join:
    /// 1. `crates_keywords.csv` from the `crates_keywords` table (`crate_id` to `keyword_id` mapping)
    /// 2. `keywords.csv` from the `keywords` table, `keyword` field (keyword strings)
    pub keywords: Vec<CompactString>,

    /// List of crate owners (users and teams with publish permissions).
    /// Each owner includes login name and optional display name.
    ///
    /// **Source**: Multi-table join:
    /// 1. `crate_owners.csv` from the `crate_owners` table (`crate_id` to owner mapping with user/team discriminant)
    /// 2. `users.csv` from the `users` table, `gh_login` and `name` fields (for user owners)
    /// 3. `teams.csv` from the `teams` table, `login` and `name` fields (for team owners)
    pub owners: Vec<Owner>,

    /// Monthly download statistics aggregated across all versions of this crate.
    /// Each tuple contains (first day of month, total downloads in that month across all versions).
    /// Data is aggregated from daily download records.
    ///
    /// **Source**: `version_downloads.csv` from the `version_downloads` table
    /// - Aggregated across all versions of this crate
    /// - Aggregated by (year, month) to produce monthly totals
    /// - Sorted chronologically
    pub monthly_downloads: Vec<(NaiveDate, u64)>,

    /// Total all-time download count for this crate (across all versions).
    ///
    /// **Source**: `crate_downloads.csv` from the `crate_downloads` table, `downloads` field
    pub downloads: u64,

    /// Number of unique crates that depend on this crate (as counted from the database dump).
    /// This represents how many other crates list this one as a dependency.
    ///
    /// **Source**: Computed from multi-table join:
    /// 1. `dependencies.csv` from the `dependencies` table (find all `version_ids` that depend on this `crate_id`)
    /// 2. `versions.csv` from the `versions` table (map `version_ids` back to their `crate_ids`)
    /// 3. Count unique dependent `crate_ids`
    pub dependents: u64,

    /// Number of different versions of this crate published within the last 90 days.
    /// This helps assess the release frequency and stability of the crate.
    ///
    /// **Source**: Computed from `versions.csv` from the `versions` table
    /// - Count versions for this `crate_id` where `created_at` is within the last 90 days
    pub versions_last_90_days: u64,
}
