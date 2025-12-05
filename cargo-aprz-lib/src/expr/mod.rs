//! Expression-based crate evaluation using CEL
//!
//! This module implements the policy evaluation engine that determines whether
//! crates should be accepted or denied based on user-defined expressions. It
//! uses the CEL (Common Expression Language) to provide a safe, sandboxed
//! evaluation environment.
//!
//! # Implementation Model
//!
//! The evaluation process follows a three-tier policy model:
//!
//! 1. **Deny-if-any**: If any expression evaluates to true, immediately deny
//! 2. **Accept-if-any**: If any expression evaluates to true, accept (unless denied)
//! 3. **Accept-if-all**: Accept only if all expressions evaluate to true (unless denied)
//!
//! Each tier contains a list of [`Expression`] objects parsed from user configuration.
//! Expressions are compiled once at startup for efficiency and validated to ensure
//! they reference only valid metric names.
//!
//! The [`evaluate`] function is the main entry point. For each crate, it:
//! - Builds a CEL context with all metric values as variables
//! - Adds a `now` variable for datetime comparisons
//! - Evaluates expressions in order (deny-if-any, then accept-if-any, then accept-if-all)
//! - Returns an [`EvaluationOutcome`] with the final acceptance status and reasons
//!
//! The CEL context is created once per crate and reused for all expressions,
//! significantly improving performance when evaluating multiple expressions.

mod evaluation_outcome;
mod evaluator;
mod expression;

pub use evaluation_outcome::EvaluationOutcome;
pub use evaluator::evaluate;
pub use expression::Expression;
