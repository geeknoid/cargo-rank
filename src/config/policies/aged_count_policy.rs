use crate::config::policy::Policy;
use crate::metrics::Metric;
use crate::misc::DependencyTypes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgedCountPolicy {
    #[serde(default)]
    pub dependency_types: DependencyTypes,
    pub min_count: u32,
    pub max_days: u32,
    pub points: f64,
}

impl Policy for AgedCountPolicy {
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

                if policy_a.min_count <= policy_b.min_count && policy_a.max_days >= policy_b.max_days {
                    warnings.push(format!(
                        "{metric}: Policy #{i} dominates policy #{j} for dependency types '{overlap}'"
                    ));
                }
            }
        }

        warnings
    }
}
