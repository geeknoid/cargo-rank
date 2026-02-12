use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum ExpressionDisposition {
    True,
    False,
    Failed(String),
}

/// The outcome of evaluating a single expression.
#[derive(Debug, Clone)]
pub struct ExpressionOutcome {
    pub name: Arc<str>,
    pub description: Arc<str>,
    pub disposition: ExpressionDisposition,
}

impl ExpressionOutcome {
    #[must_use]
    #[expect(clippy::missing_const_for_fn, reason = "Arc<str> parameters prevent const")]
    pub fn new(name: Arc<str>, description: Arc<str>, disposition: ExpressionDisposition) -> Self {
        Self {
            name,
            description,
            disposition,
        }
    }
}

