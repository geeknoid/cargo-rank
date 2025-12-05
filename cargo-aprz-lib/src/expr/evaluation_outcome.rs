/// The outcome of evaluating a crate against policy expressions.
#[derive(Debug, Clone)]
pub struct EvaluationOutcome {
    pub accepted: bool,
    pub reasons: Vec<String>,
}

impl EvaluationOutcome {
    #[must_use]
    pub const fn new(accepted: bool, reasons: Vec<String>) -> Self {
        Self { accepted, reasons }
    }
}
