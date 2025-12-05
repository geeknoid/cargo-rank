//! Scoring logic for evaluating crate quality.

use crate::config::Config;
use crate::facts::CrateFacts;
use crate::metrics::{Metric, MetricCategory};
use crate::misc::DependencyType;
use crate::ranking::{PolicyOutcome, metric_calculator};
use compact_str::CompactString;
use core::cell::RefCell;
use core::hash::BuildHasher;
use std::collections::HashMap;

/// Result of ranking a crate
#[derive(Debug)]
pub struct RankingOutcome {
    pub overall_score: f64,
    pub category_scores: HashMap<MetricCategory, f64>,
    pub details: HashMap<Metric, PolicyOutcome>,
    pub dependency_type: DependencyType,
}

/// Ranker evaluates crate quality based on configured policies
#[derive(Debug)]
pub struct Ranker<'a> {
    config: &'a Config,
    policy_outcomes: RefCell<HashMap<Metric, PolicyOutcome>>,
}

impl<'a> Ranker<'a> {
    /// Create a new ranker with the given configuration
    #[must_use]
    pub fn new(config: &'a Config) -> Self {
        Self {
            config,
            policy_outcomes: RefCell::new(HashMap::new()),
        }
    }

    /// Rank a crate based on multiple quality criteria
    pub fn rank(&self, facts: &CrateFacts, dependency_type: DependencyType) -> RankingOutcome {
        let mut policy_outcomes = self.policy_outcomes.borrow_mut();

        policy_outcomes.clear();
        metric_calculator::calculate(self.config, facts, dependency_type, &mut policy_outcomes);

        let mut total_points = 0.0;
        let mut category_points: HashMap<MetricCategory, f64> = HashMap::new();
        let mut category_counts: HashMap<MetricCategory, usize> = HashMap::new();

        for (metric, outcome) in policy_outcomes.iter() {
            let category = metric.category();
            let points = match outcome {
                PolicyOutcome::Match(points, _info) => *points,
                PolicyOutcome::NoMatch(_reason) => 0.0,
            };
            total_points += points;
            *category_points.entry(category).or_insert(0.0) += points;
            *category_counts.entry(category).or_insert(0) += 1;
        }

        let score = if policy_outcomes.is_empty() {
            0.0
        } else {
            #[expect(clippy::cast_precision_loss, reason = "Precision loss acceptable for score calculation")]
            let avg = total_points / policy_outcomes.len() as f64;
            (avg * 100.0).round() / 100.0
        };

        // Compute average score per category
        let mut category_scores = HashMap::new();
        for (category, total_points) in category_points {
            if let Some(&count) = category_counts.get(&category)
                && count > 0
            {
                #[expect(clippy::cast_precision_loss, reason = "Precision loss acceptable for score calculation")]
                let avg = total_points / count as f64;
                let rounded = (avg * 100.0).round() / 100.0;
                _ = category_scores.insert(category, rounded);
            }
        }

        RankingOutcome {
            overall_score: score,
            category_scores,
            details: policy_outcomes.clone(),
            dependency_type,
        }
    }
}

/// Extract failure reasons from policy results
#[must_use]
pub fn extract_reasons<S: BuildHasher>(details: &HashMap<Metric, PolicyOutcome, S>) -> Vec<CompactString> {
    let mut reasons = Vec::new();
    for outcome in details.values() {
        if let PolicyOutcome::NoMatch(reason) = outcome {
            reasons.push(reason.clone());
        }
    }
    reasons
}
