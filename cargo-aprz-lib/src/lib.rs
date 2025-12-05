#![doc(hidden)]
#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

//! Core library for cargo-aprz
//!
//! This library consolidates all functionality for the cargo-aprz tool, which analyzes
//! Rust crates for compliance with user-defined policies.
//!
//! # Module Organization
//!
//! - [`commands`]: Command-line interface and orchestration
//! - [`facts`]: Data collection and aggregation
//! - [`metrics`]: Metric extraction from facts
//! - [`expr`]: Expression-based evaluation
//! - [`reports`]: Report generation in multiple formats

pub type Result<T, E = ohno::AppError> = core::result::Result<T, E>;

#[cfg(any(debug_assertions, test))]
pub mod commands;
#[cfg(not(any(debug_assertions, test)))]
mod commands;

#[cfg(any(debug_assertions, test))]
pub mod expr;
#[cfg(not(any(debug_assertions, test)))]
mod expr;

#[cfg(any(debug_assertions, test))]
pub mod facts;
#[cfg(not(any(debug_assertions, test)))]
mod facts;

#[cfg(any(debug_assertions, test))]
pub mod metrics;
#[cfg(not(any(debug_assertions, test)))]
mod metrics;

#[cfg(any(debug_assertions, test))]
pub mod reports;
#[cfg(not(any(debug_assertions, test)))]
mod reports;

pub use crate::commands::{Host, run};
