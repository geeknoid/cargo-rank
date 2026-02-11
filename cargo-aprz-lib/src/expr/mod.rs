//! Expression-based crate evaluation using CEL
//!
//! This module implements the policy evaluation engine that determines the risk
//! level of crates based on user-defined expressions. It uses the CEL (Common
//! Expression Language) to provide a safe, sandboxed evaluation environment.
//!
//! # Implementation Model
//!
//! The evaluation process follows a two-tier policy model:
//!
//! 1. **High-risk-if-any**: If any expression evaluates to true, flag as high risk
//! 2. **Eval**: Each expression has a point value (default 1). All expressions are
//!    evaluated and a score is computed as `granted_points / total_points * 100`.
//!    The score is compared against configurable thresholds to determine the risk level.
//!
//! Each tier contains a list of [`Expression`] objects parsed from user configuration.
//! Expressions are compiled once at startup for efficiency and validated to ensure
//! they reference only valid metric names.
//!
//! The [`evaluate`] function is the main entry point. For each crate, it:
//! - Builds a CEL context with all metric values as variables
//! - Evaluates expressions in order (high-risk-if-any, then eval)
//! - Returns an [`Appraisal`] with the risk level, score, and reasons
//!
//! The CEL context is created once per crate and reused for all expressions,
//! significantly improving performance when evaluating multiple expressions.

mod appraisal;
mod evaluator;
mod expression;
mod expression_outcome;

pub use appraisal::{Appraisal, Risk};
pub use evaluator::evaluate;
pub use expression::Expression;
pub use expression_outcome::ExpressionOutcome;
