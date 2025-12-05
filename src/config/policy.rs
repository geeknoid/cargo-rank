//! Common trait for policy validation

use crate::metrics::Metric;
use crate::misc::DependencyTypes;

/// Common interface for all policy types
pub trait Policy: Sized {
    /// Get the dependency types this policy applies to
    fn dependency_types(&self) -> &DependencyTypes;

    /// Get the score for this policy
    fn points(&self) -> f64;

    /// Validate policies and return warnings
    fn validate<'a>(metric: Metric, policies: impl IntoIterator<Item = &'a Self>) -> Vec<String>
    where
        Self: 'a;
}
