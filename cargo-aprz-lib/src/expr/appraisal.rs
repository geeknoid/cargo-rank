use super::{ExpressionOutcome, Risk};

/// The outcome of evaluating a crate against policy expressions.
#[derive(Debug, Clone)]
pub struct Appraisal {
    pub risk: Risk,
    pub expression_outcomes: Vec<ExpressionOutcome>,
    pub available_points: u32,
    pub awarded_points: u32,
    pub score: f64,
}

impl Appraisal {
    #[must_use]
    pub const fn new(
        risk: Risk,
        expression_outcomes: Vec<ExpressionOutcome>,
        available_points: u32,
        awarded_points: u32,
        score: f64,
    ) -> Self {
        Self {
            risk,
            expression_outcomes,
            available_points,
            awarded_points,
            score,
        }
    }
}
