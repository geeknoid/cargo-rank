//! cargo-rank crate
//!
//! This crate is an implementation detail of the `cargo-rank` tool. This crate's API is fluid and may change without warning
//! and in a semver-incompatible way.

/// Result type alias using `ohno::AppError` as the default error type.
pub type Result<T, E = ohno::AppError> = core::result::Result<T, E>;

#[doc(hidden)]
pub mod config;

#[doc(hidden)]
pub mod facts;

#[doc(hidden)]
pub mod metrics;

#[doc(hidden)]
pub mod misc;

#[doc(hidden)]
pub mod ranking;

#[doc(hidden)]
pub mod reports;
