//! Client for working with crates.io database dumps.
//!
//! This module provides functionality to download and query the official
//! crates.io database dump instead of using the API.

mod crate_overall_data;
mod crate_version_data;
mod owner;
mod owner_kind;
mod provider;
mod rust_edition;
mod tables;

pub use crate_overall_data::CrateOverallData;
pub use crate_version_data::CrateVersionData;
pub use owner_kind::OwnerKind;
pub use provider::Provider;
