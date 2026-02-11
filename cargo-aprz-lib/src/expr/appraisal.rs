use super::ExpressionOutcome;

/// The risk level assigned to a crate after policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Risk {
    Low,
    Medium,
    High,
}

impl core::fmt::Display for Risk {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Low => write!(f, "LOW RISK"),
            Self::Medium => write!(f, "MEDIUM RISK"),
            Self::High => write!(f, "HIGH RISK"),
        }
    }
}

/// The outcome of evaluating a crate against policy expressions.
#[derive(Debug, Clone)]
pub struct Appraisal {
    pub risk: Risk,
    pub expression_outcomes: Vec<ExpressionOutcome>,
}

impl Appraisal {
    #[must_use]
    pub const fn new(risk: Risk, expression_outcomes: Vec<ExpressionOutcome>) -> Self {
        Self {
            risk,
            expression_outcomes,
        }
    }
}
