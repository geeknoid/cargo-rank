use crate::config::policy::Policy;
use crate::metrics::Metric;
use crate::misc::DependencyTypes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ResponsivenessPolicy {
    #[serde(default)]
    pub dependency_types: DependencyTypes,

    pub max_average_days: u32,
    pub max_p50_days: u32,
    pub max_p75_days: u32,
    pub max_p90_days: u32,
    pub max_p95_days: u32,

    pub points: f64,
}

impl Policy for ResponsivenessPolicy {
    fn dependency_types(&self) -> &DependencyTypes {
        &self.dependency_types
    }

    fn points(&self) -> f64 {
        self.points
    }

    fn validate<'a>(metric: Metric, policies: impl IntoIterator<Item = &'a Self>) -> Vec<String>
    where
        Self: 'a,
    {
        let mut warnings = Vec::new();
        let policies: Vec<_> = policies.into_iter().collect();

        for (i, policy_a) in policies.iter().enumerate() {
            for (j, policy_b) in policies.iter().enumerate().skip(i + 1) {
                let policy_a = *policy_a;
                let policy_b = *policy_b;
                let overlap = policy_a.dependency_types().intersect(policy_b.dependency_types());
                if overlap.is_empty() {
                    continue;
                }

                // Check if policy_a dominates policy_b (all thresholds are >= policy_b's thresholds)
                if policy_a.max_average_days >= policy_b.max_average_days
                    && policy_a.max_p50_days >= policy_b.max_p50_days
                    && policy_a.max_p75_days >= policy_b.max_p75_days
                    && policy_a.max_p90_days >= policy_b.max_p90_days
                    && policy_a.max_p95_days >= policy_b.max_p95_days
                {
                    warnings.push(format!(
                        "{metric}: Policy #{i} dominates policy #{j} for dependency type '{overlap}'"
                    ));
                }
            }
        }

        warnings
    }
}
