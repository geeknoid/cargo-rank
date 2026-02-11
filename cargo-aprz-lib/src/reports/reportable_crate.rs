use crate::expr::Appraisal;
use crate::metrics::Metric;
use semver::Version;
use std::sync::Arc;

/// A crate with its metrics and optional evaluation outcome, ready for reporting.
#[derive(Debug, Clone)]
pub struct ReportableCrate {
    pub name: Arc<str>,
    pub version: Arc<Version>,
    pub metrics: Vec<Metric>,
    pub appraisal: Option<Appraisal>,
}

impl ReportableCrate {
    #[must_use]
    #[expect(clippy::missing_const_for_fn, reason = "Cannot be const due to non-const parameter types")]
    pub fn new(name: Arc<str>, version: Arc<Version>, metrics: Vec<Metric>, appraisal: Option<Appraisal>) -> Self {
        Self {
            name,
            version,
            metrics,
            appraisal,
        }
    }
}
