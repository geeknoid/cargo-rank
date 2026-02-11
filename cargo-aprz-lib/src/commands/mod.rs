//! Command-line interface and orchestration for cargo-aprz
//!
//! This module implements the CLI commands and coordinates all the other crates
//! to perform end-to-end crate analysis, evaluation, and reporting. It handles
//! argument parsing, configuration management, and the high-level workflows.
//!
//! # Implementation Model
//!
//! The module is organized around four main commands:
//!
//! ## Commands
//!
//! - **crates**: Analyze specific crates by name/version, collect facts, evaluate
//!   against policy expressions, and generate reports
//! - **deps**: Analyze all dependencies in a workspace, similar to crates command
//!   but automatically discovers crates from Cargo.lock
//! - **init**: Generate a default configuration file with example expressions
//! - **validate**: Check configuration file syntax and expression validity
//!
//! ## Execution Flow
//!
//! The `run` function parses command-line arguments using clap and routes
//! to the appropriate command handler. Each command follows a similar pattern:
//!
//! 1. Parse arguments and load configuration
//! 2. Collect crate facts using cargo-aprz-facts
//! 3. Convert facts to metrics using cargo-aprz-metrics
//! 4. Optionally evaluate using cargo-aprz-expr
//! 5. Generate reports using cargo-aprz-reports
//!
//! The `common` module provides shared functionality like logging setup,
//! color mode handling, and the main report generation logic that coordinates
//! multiple output formats.
//!
//! Configuration is managed through a TOML file with two expression lists
//! (`high_risk_if_any`, `eval`) that define the evaluation policy.

mod common;
mod config;
mod crates;
mod deps;
mod host;
mod init;
mod progress_reporter;
mod run;
mod validate;

#[cfg(debug_assertions)]
pub use config::Config;

pub use crates::{CratesArgs, process_crates};
pub use deps::{DepsArgs, process_dependencies};
pub use host::Host;
pub use init::{InitArgs, init_config};
pub use progress_reporter::ProgressReporter;
pub use run::run;
pub use validate::{ValidateArgs, validate_config};
