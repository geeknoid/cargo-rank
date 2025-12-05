//! Request tracking for monitoring outstanding HTTP requests.

use crate::facts::progress_reporter::ProgressReporter;
use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Topics that can be tracked for progress reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrackedTopic {
    Coverage,
    Docs,
    GitHub,
    Repos,
}

impl TrackedTopic {
    /// Get the display name for this topic.
    const fn name(self) -> &'static str {
        match self {
            Self::Coverage => "coverage",
            Self::Docs => "docs",
            Self::GitHub => "github",
            Self::Repos => "repos",
        }
    }

    /// Get all tracked topics in a consistent order.
    const fn all() -> [Self; 4] {
        [Self::Coverage, Self::Docs, Self::GitHub, Self::Repos]
    }

    /// Convert to array index.
    const fn index(self) -> usize {
        self as usize
    }
}

/// Counter for a specific tracked topic.
#[derive(Debug, Default)]
struct RequestCounter {
    issued: AtomicU64,
    completed: AtomicU64,
}

/// Tracks outstanding requests and updates progress reporting.
///
/// This is used to monitor requests to external services like GitHub, docs.rs,
/// and codecov.io, providing visibility into the query phase of crate analysis.
///
/// Requests are tracked by topic, with separate counters for different request types.
#[derive(Debug, Clone)]
pub struct RequestTracker {
    counters: Arc<[RequestCounter; 4]>,
}

impl RequestTracker {
    /// Create a new request tracker with the given progress reporter.
    #[must_use]
    pub fn new(progress: &ProgressReporter) -> Self {
        let counters = Arc::new([
            RequestCounter::default(),
            RequestCounter::default(),
            RequestCounter::default(),
            RequestCounter::default(),
        ]);

        // Register a callback that computes progress state on-demand
        let counters_clone = Arc::clone(&counters);
        progress.configure_determinate(move || Self::progress_reporter_callback(&counters_clone));

        Self { counters }
    }

    /// Mark that multiple new requests have been issued for the given topic.
    pub fn add_requests(&self, topic: TrackedTopic, count: u64) {
        let counter = &self.counters[topic.index()];
        let _ = counter.issued.fetch_add(count, Ordering::Relaxed);
    }

    /// Mark that a request has completed for the given topic.
    pub fn complete_request(&self, topic: TrackedTopic) {
        let counter = &self.counters[topic.index()];
        let _ = counter.completed.fetch_add(1, Ordering::Relaxed);
    }

    /// Compute current progress state from counters.
    ///
    /// Returns (`total_length`, `current_position`, `message_string`).
    fn progress_reporter_callback(counters: &[RequestCounter; 4]) -> (u64, u64, String) {
        let mut total_issued = 0u64;
        let mut total_completed = 0u64;
        let mut parts = Vec::new();

        for topic in TrackedTopic::all() {
            let counter = &counters[topic.index()];
            let issued = counter.issued.load(Ordering::Relaxed);
            let completed = counter.completed.load(Ordering::Relaxed);

            if issued > 0 {
                total_issued += issued;
                total_completed += completed;
                parts.push(format!("{completed}/{issued} {}", topic.name()));
            }
        }

        let message = if parts.is_empty() {
            "No requests".to_string()
        } else {
            parts.join(", ")
        };

        (total_issued, total_completed, message)
    }
}
