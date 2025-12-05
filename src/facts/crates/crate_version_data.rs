use super::rust_edition::RustEdition;
use chrono::{DateTime, Utc};
use semver::Version;
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
    /// Timestamp when this crate data was collected from the crates.io database.
    ///
    /// **Source**: Database sync timestamp
    pub timestamp: DateTime<Utc>,

    /// The semantic version number being queried (e.g., "1.2.3").
    ///
    /// **Source**: User-provided request (`CrateSpec`)
    pub version: Version,

    /// Optional human-readable description of what this crate does.
    /// This is the text that appears in search results and on the crate's crates.io page.
    ///
    /// **Source**: `versions.csv` → `versions` table → `description` field
    pub description: Option<String>,

    /// Optional URL to the crate's homepage (may differ from repository).
    /// Often points to project documentation or landing pages.
    ///
    /// **Source**: `versions.csv` → `versions` table → `homepage` field
    pub homepage: Option<Url>,

    /// Optional URL to the crate's documentation.
    /// If not specified, defaults to docs.rs for published crates.
    ///
    /// **Source**: `versions.csv` → `versions` table → `documentation` field
    pub documentation: Option<Url>,

    /// Optional SPDX license identifier or expression (e.g., "MIT", "Apache-2.0 OR MIT").
    /// Indicates the license(s) under which this version is distributed.
    ///
    /// **Source**: `versions.csv` → `versions` table → `license` field
    pub license: Option<String>,

    /// Optional minimum Rust version (MSRV) required to compile this crate.
    /// Format is a semantic version string (e.g., "1.70.0").
    ///
    /// **Source**: `versions.csv` → `versions` table → `rust_version` field
    pub rust_version: Option<String>,

    /// Optional Rust edition this crate targets (e.g., Edition2021, Edition2024).
    /// Determines which language features and deprecations apply.
    ///
    /// **Source**: `versions.csv` → `versions` table → `edition` field
    /// - Parsed from string representation to `RustEdition` enum
    pub edition: Option<RustEdition>,

    /// Cargo features defined in this version's Cargo.toml.
    /// Maps feature names to their dependencies (other features or crates they enable).
    ///
    /// **Source**: `versions.csv` → `versions` table → `features` field
    /// - Stored as JSON in the database, deserialized to `BTreeMap`
    pub features: BTreeMap<String, Vec<String>>,

    /// When this specific version was first published to crates.io.
    ///
    /// **Source**: `versions.csv` → `versions` table → `created_at` field
    pub created_at: DateTime<Utc>,

    /// When this version's metadata was last updated.
    /// May differ from `created_at` if republished or metadata was modified.
    ///
    /// **Source**: `versions.csv` → `versions` table → `updated_at` field
    pub updated_at: DateTime<Utc>,

    /// Whether this version has been yanked from crates.io.
    /// Yanked versions are hidden from resolution but remain downloadable by exact version.
    ///
    /// **Source**: `versions.csv` → `versions` table → `yanked` field
    pub yanked: bool,

    /// Total download count for this specific version.
    ///
    /// **Source**: `versions.csv` → `versions` table → `downloads` field
    pub downloads: u64,
}
