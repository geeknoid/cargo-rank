use crate::expr::EvaluationOutcome;
use crate::metrics::Metric;
use semver::Version;

/// A crate with its metrics and optional evaluation outcome, ready for reporting.
#[derive(Debug, Clone)]
pub struct ReportableCrate {
    pub name: String,
    pub version: Version,
    pub metrics: Vec<Metric>,
    pub evaluation: Option<EvaluationOutcome>,
}

impl ReportableCrate {
    #[must_use]
    #[expect(clippy::missing_const_for_fn, reason = "Cannot be const due to non-const parameter types")]
    pub fn new(name: String, version: Version, metrics: Vec<Metric>, evaluation: Option<EvaluationOutcome>) -> Self {
        Self {
            name,
            version,
            metrics,
            evaluation,
        }
    }
}
