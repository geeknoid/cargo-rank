use super::rust_edition::RustEdition;
use chrono::{DateTime, NaiveDate, Utc};
use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;

/// Version-specific crate information.
///
/// This struct contains metadata that is specific to a particular version of a crate.
/// Different versions of the same crate may have different descriptions, features, editions, etc.
/// All data originates from the crates.io database dump, specifically the `versions` table.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CrateVersionData {
    /// Optional human-readable description of what this crate does.
    /// This is the text that appears in search results and on the crate's crates.io page.
    ///
    /// **Source**: `versions.csv` from the `versions` table, `description` field
    pub description: CompactString,

    /// Optional URL to the crate's homepage (may differ from repository).
    /// Often points to project documentation or landing pages.
    ///
    /// **Source**: `versions.csv` from the `versions` table, `homepage` field
    pub homepage: Option<Url>,

    /// Optional URL to the crate's documentation.
    /// If not specified, defaults to docs.rs for published crates.
    ///
    /// **Source**: `versions.csv` from the `versions` table, `documentation` field
    pub documentation: Option<Url>,

    /// Optional SPDX license identifier or expression (e.g., "MIT", "Apache-2.0 OR MIT").
    /// Indicates the license(s) under which this version is distributed.
    ///
    /// **Source**: `versions.csv` from the `versions` table, `license` field
    pub license: CompactString,

    /// Optional minimum Rust version (MSRV) required to compile this crate.
    /// Format is a semantic version string (e.g., "1.70.0").
    ///
    /// **Source**: `versions.csv` from the `versions` table, `rust_version` field
    pub rust_version: CompactString,

    /// Optional Rust edition this crate targets.
    /// Determines which language features and deprecations apply.
    ///
    /// **Source**: `versions.csv` from the `versions` table, `edition` field
    /// - Parsed from string representation to `RustEdition` enum
    pub edition: Option<RustEdition>,

    /// Cargo features defined in this version's Cargo.toml.
    /// Maps feature names to their dependencies (other features or crates they enable).
    ///
    /// **Source**: `versions.csv` from the `versions` table, `features` field
    /// - Stored as JSON in the database, deserialized to `BTreeMap`
    pub features: BTreeMap<CompactString, Vec<CompactString>>,

    /// When this specific version was first published to crates.io.
    ///
    /// **Source**: `versions.csv` from the `versions` table, `created_at` field
    pub created_at: DateTime<Utc>,

    /// When this version's metadata was last updated.
    /// May differ from `created_at` if republished or metadata was modified.
    ///
    /// **Source**: `versions.csv` from the `versions` table, `updated_at` field
    pub updated_at: DateTime<Utc>,

    /// Whether this version has been yanked from crates.io.
    /// Yanked versions are hidden from resolution but remain downloadable by exact version.
    ///
    /// **Source**: `versions.csv` from the `versions` table, `yanked` field
    pub yanked: bool,

    /// Total download count for this specific version.
    ///
    /// **Source**: `versions.csv` from the `versions` table, `downloads` field
    pub downloads: u64,

    /// Monthly download statistics for this specific version.
    /// Each tuple contains (first day of month, total downloads in that month).
    /// Data is aggregated from daily download records.
    ///
    /// **Source**: `version_downloads.csv` from the `version_downloads` table
    /// - Filtered by `version_id` (for this specific version)
    /// - Aggregated by (year, month) to produce monthly totals
    /// - Sorted chronologically
    pub monthly_downloads: Vec<(NaiveDate, u64)>,
}
