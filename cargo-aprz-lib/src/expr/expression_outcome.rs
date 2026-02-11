use core::fmt;

/// The outcome of evaluating a single expression.
#[derive(Debug, Clone)]
pub struct ExpressionOutcome {
    pub name: String,
    pub description: String,
    pub result: bool,
}

impl ExpressionOutcome {
    #[must_use]
    pub const fn new(name: String, description: String, result: bool) -> Self {
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
