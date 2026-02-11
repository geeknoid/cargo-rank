use core::fmt;
use std::sync::Arc;

/// The outcome of evaluating a single expression.
#[derive(Debug, Clone)]
pub struct ExpressionOutcome {
    pub name: Arc<str>,
    pub description: Arc<str>,
    pub result: bool,
}

impl ExpressionOutcome {
    #[must_use]
    #[expect(clippy::missing_const_for_fn, reason = "Arc<str> parameters prevent const")]
    pub fn new(name: Arc<str>, description: Arc<str>, result: bool) -> Self {
        Self {
            name,
            description,
            result,
        }
    }
}

impl fmt::Display for ExpressionOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.result {
            write!(f, "{}: {}", self.name, self.description)
        } else {
            write!(f, "{} (failed): {}", self.name, self.description)
        }
    }
}
