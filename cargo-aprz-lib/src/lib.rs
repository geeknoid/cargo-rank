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

macro_rules! declare_modules {
    ($($mod:ident),+ $(,)?) => {
        $(
            #[cfg(debug_assertions)]
            pub mod $mod;
            #[cfg(not(debug_assertions))]
            mod $mod;
        )+
    };
}

declare_modules!(commands, expr, facts, metrics, reports);

pub use crate::commands::{Host, run};
