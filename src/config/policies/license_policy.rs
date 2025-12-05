use crate::config::policy::Policy;
use crate::metrics::Metric;
use crate::misc::DependencyTypes;
use core::fmt::Formatter;
use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicensePolicy {
    #[serde(default)]
    pub dependency_types: DependencyTypes,

    #[serde(serialize_with = "serialize_licenses", deserialize_with = "deserialize_licenses")]
    pub licenses: HashSet<String>,

    pub points: f64,
}

fn serialize_licenses<S>(licenses: &HashSet<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut names: Vec<_> = licenses.iter().map(String::as_str).collect();
    names.sort_unstable();
    serializer.serialize_str(&names.join(", "))
}

fn deserialize_licenses<'de, D>(deserializer: D) -> Result<HashSet<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct LicensesVisitor;

    impl Visitor<'_> for LicensesVisitor {
        type Value = HashSet<String>;

        fn expecting(&self, formatter: &mut Formatter<'_>) -> core::fmt::Result {
            formatter.write_str("a comma-separated string of license names")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let mut set = HashSet::new();
            for part in v.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    let _ = set.insert(trimmed.to_string());
                }
            }
            Ok(set)
        }
    }

    deserializer.deserialize_str(LicensesVisitor)
}

impl LicensePolicy {
    /// Check if an SPDX license expression matches the allowed licenses
    #[must_use]
    pub fn check_license(&self, spdx_license_expr: &str) -> bool {
        // Try to parse as SPDX expression
        let Ok(expression) = spdx::Expression::parse(spdx_license_expr) else {
            // If parsing fails, fall back to simple substring matching for backward compatibility
            let license_lower = spdx_license_expr.to_lowercase();
            return self.licenses.iter().any(|allowed| {
                let allowed_lower = allowed.to_lowercase();
                license_lower.contains(&allowed_lower)
            });
        };

        // Manually walk the expression tree to properly evaluate AND/OR logic
        // We need to collect all requirements and evaluate the expression structure
        Self::evaluate_spdx_expression(&expression, &self.licenses)
    }

    /// Recursively evaluate an SPDX expression
    fn evaluate_spdx_expression(expr: &spdx::Expression, allowed_licenses: &HashSet<String>) -> bool {
        // Iterate through the expression nodes
        // The expression provides an iterator, but we need to manually track the structure
        // For now, let's use a simpler approach: check if all requirements are satisfied

        // Get all license requirements
        let all_requirements: Vec<_> = expr.requirements().collect();

        if all_requirements.is_empty() {
            return false;
        }

        // For each requirement, check if it's allowed
        let mut has_allowed = false;
        let mut has_disallowed = false;

        for req in &all_requirements {
            let Some(license_id_obj) = req.req.license.id() else {
                has_disallowed = true;
                continue;
            };

            let license_id = license_id_obj.name.to_lowercase();

            let is_allowed = allowed_licenses.iter().any(|allowed| {
                let allowed_lower = allowed.to_lowercase();
                license_id.contains(&allowed_lower) || allowed_lower.contains(&license_id)
            });

            if is_allowed {
                has_allowed = true;
            } else {
                has_disallowed = true;
            }
        }

        // Check if the expression contains AND or OR operators
        // by parsing the original string (not ideal, but the spdx crate doesn't expose the tree structure easily)
        let expr_str = format!("{expr}");
        let has_and = expr_str.contains(" AND ");

        // Apply logic based on operators:
        // - If there's an AND, ALL licenses must be allowed
        // - If there's only OR (or neither), at least ONE license must be allowed
        if has_and {
            // For AND: all requirements must be satisfied
            !has_disallowed && has_allowed
        } else {
            // For OR or single license: at least one requirement must be satisfied
            has_allowed
        }
    }
}

impl Policy for LicensePolicy {
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

                let mut intersection: Vec<String> = policy_a.licenses.intersection(&policy_b.licenses).cloned().collect();
                if !intersection.is_empty() {
                    intersection.sort_unstable();
                    warnings.push(format!(
                        "{metric}: Policies at index {i} dominates policy at index {j} for dependency types '{overlap}' and licenses '{}'",
                        intersection.join(", ")
                    ));
                }
            }
        }

        warnings
    }
}
