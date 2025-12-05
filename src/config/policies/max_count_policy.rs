use crate::config::policy::Policy;
use crate::metrics::Metric;
use crate::misc::DependencyTypes;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MaxCountPolicy {
    #[serde(default)]
    pub dependency_types: DependencyTypes,

    pub max_count: u32,

    pub points: f64,
}

impl Policy for MaxCountPolicy {
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

                if policy_a.max_count >= policy_b.max_count {
                    warnings.push(format!(
                        "{metric}: Policy #{i} dominates policy #{j} for dependency types '{overlap}'"
                    ));
                }
            }
        }

        warnings
    }
}
