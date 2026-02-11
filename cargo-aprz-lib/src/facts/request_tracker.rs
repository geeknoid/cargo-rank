//! Request tracking for monitoring outstanding HTTP requests.

use super::progress::Progress;
use core::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Topics that can be tracked for progress reporting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrackedTopic {
    Coverage,
    Docs,
    Repos,
    Codebase,
}

impl TrackedTopic {
    /// Get the display name for this topic.
    const fn name(self) -> &'static str {
        match self {
            Self::Coverage => "coverage",
            Self::Docs => "docs",
            Self::Repos => "repos",
            Self::Codebase => "codebase",
        }
    }

    /// Get all tracked topics in a consistent order.
    const fn all() -> [Self; 4] {
        [Self::Coverage, Self::Docs, Self::Repos, Self::Codebase]
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
/// and `codecov.io`, providing visibility into the query phase of crate analysis.
///
/// Requests are tracked by topic, with separate counters for different request types.
#[derive(Debug, Clone, Default)]
pub struct RequestTracker {
    counters: Arc<[RequestCounter; 4]>,
}

impl RequestTracker {
    /// Create a new request tracker with the given progress reporter.
    #[must_use]
    pub fn new(progress: &dyn Progress) -> Self {
        let result = Self::default();

        let counters_clone = Arc::clone(&result.counters);
        progress.set_determinate(Box::new(move || Self::progress_reporter_callback(&counters_clone)));

        result
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
        let mut parts = Vec::with_capacity(TrackedTopic::all().len());

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracked_topic_name() {
        assert_eq!(TrackedTopic::Coverage.name(), "coverage");
        assert_eq!(TrackedTopic::Docs.name(), "docs");
        assert_eq!(TrackedTopic::Repos.name(), "repos");
        assert_eq!(TrackedTopic::Codebase.name(), "codebase");
    }

    #[test]
    fn test_tracked_topic_all() {
        let all_topics = TrackedTopic::all();
        assert_eq!(all_topics.len(), 4);
        assert_eq!(all_topics[0], TrackedTopic::Coverage);
        assert_eq!(all_topics[1], TrackedTopic::Docs);
        assert_eq!(all_topics[2], TrackedTopic::Repos);
        assert_eq!(all_topics[3], TrackedTopic::Codebase);
    }

    #[test]
    fn test_tracked_topic_index() {
        assert_eq!(TrackedTopic::Coverage.index(), 0);
        assert_eq!(TrackedTopic::Docs.index(), 1);
        assert_eq!(TrackedTopic::Repos.index(), 2);
        assert_eq!(TrackedTopic::Codebase.index(), 3);
    }

    #[test]
    fn test_add_single_request() {
        let tracker = RequestTracker::default();
        tracker.add_requests(TrackedTopic::Coverage, 1);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters);

        assert_eq!(total, 1);
        assert_eq!(completed, 0);
        assert_eq!(message, "0/1 coverage");
    }

    #[test]
    fn test_add_multiple_requests() {
        let tracker = RequestTracker::default();
        tracker.add_requests(TrackedTopic::Docs, 5);
        tracker.add_requests(TrackedTopic::Repos, 3);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters);

        assert_eq!(total, 8);
        assert_eq!(completed, 0);
        assert!(message.contains("0/5 docs"));
        assert!(message.contains("0/3 repos"));
    }

    #[test]
    fn test_complete_request() {
        let tracker = RequestTracker::default();
        tracker.add_requests(TrackedTopic::Coverage, 3);
        tracker.complete_request(TrackedTopic::Coverage);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters);

        assert_eq!(total, 3);
        assert_eq!(completed, 1);
        assert_eq!(message, "1/3 coverage");
    }

    #[test]
    fn test_complete_multiple_requests() {
        let tracker = RequestTracker::default();
        tracker.add_requests(TrackedTopic::Codebase, 5);
        tracker.complete_request(TrackedTopic::Codebase);
        tracker.complete_request(TrackedTopic::Codebase);
        tracker.complete_request(TrackedTopic::Codebase);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters);

        assert_eq!(total, 5);
        assert_eq!(completed, 3);
        assert_eq!(message, "3/5 codebase");
    }

    #[test]
    fn test_all_requests_completed() {
        let tracker = RequestTracker::default();
        tracker.add_requests(TrackedTopic::Docs, 2);
        tracker.complete_request(TrackedTopic::Docs);
        tracker.complete_request(TrackedTopic::Docs);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters);

        assert_eq!(total, 2);
        assert_eq!(completed, 2);
        assert_eq!(message, "2/2 docs");
    }

    #[test]
    fn test_multiple_topics_mixed_progress() {
        let tracker = RequestTracker::default();
        tracker.add_requests(TrackedTopic::Coverage, 10);
        tracker.add_requests(TrackedTopic::Docs, 5);
        tracker.add_requests(TrackedTopic::Repos, 3);
        tracker.add_requests(TrackedTopic::Codebase, 7);

        tracker.complete_request(TrackedTopic::Coverage);
        tracker.complete_request(TrackedTopic::Coverage);
        tracker.complete_request(TrackedTopic::Coverage);

        tracker.complete_request(TrackedTopic::Docs);
        tracker.complete_request(TrackedTopic::Docs);
        tracker.complete_request(TrackedTopic::Docs);
        tracker.complete_request(TrackedTopic::Docs);
        tracker.complete_request(TrackedTopic::Docs);

        tracker.complete_request(TrackedTopic::Repos);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters);

        assert_eq!(total, 25);
        assert_eq!(completed, 9);
        assert!(message.contains("3/10 coverage"));
        assert!(message.contains("5/5 docs"));
        assert!(message.contains("1/3 repos"));
        assert!(message.contains("0/7 codebase"));
    }

    #[test]
    fn test_no_requests() {
        let tracker = RequestTracker::default();

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters);

        assert_eq!(total, 0);
        assert_eq!(completed, 0);
        assert_eq!(message, "No requests");
    }

    #[test]
    fn test_add_requests_with_zero_count() {
        let tracker = RequestTracker::default();
        tracker.add_requests(TrackedTopic::Coverage, 0);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters);

        assert_eq!(total, 0);
        assert_eq!(completed, 0);
        assert_eq!(message, "No requests");
    }

    #[test]
    fn test_message_format_order() {
        let tracker = RequestTracker::default();
        // Add in reverse order to ensure output follows topic enum order
        tracker.add_requests(TrackedTopic::Codebase, 1);
        tracker.add_requests(TrackedTopic::Repos, 1);
        tracker.add_requests(TrackedTopic::Docs, 1);
        tracker.add_requests(TrackedTopic::Coverage, 1);

        let (_, _, message) = RequestTracker::progress_reporter_callback(&tracker.counters);

        // Message should follow the enum order: Coverage, Docs, Repos, Codebase
        let parts: Vec<&str> = message.split(", ").collect();
        assert_eq!(parts.len(), 4);
        assert!(parts[0].contains("coverage"));
        assert!(parts[1].contains("docs"));
        assert!(parts[2].contains("repos"));
        assert!(parts[3].contains("codebase"));
    }

    #[test]
    fn test_tracker_clone() {
        let tracker1 = RequestTracker::default();
        tracker1.add_requests(TrackedTopic::Coverage, 5);
        tracker1.complete_request(TrackedTopic::Coverage);

        let tracker2 = tracker1.clone();
        tracker2.add_requests(TrackedTopic::Docs, 3);

        // Both trackers should share the same counters
        let (total1, completed1, _) = RequestTracker::progress_reporter_callback(&tracker1.counters);
        let (total2, completed2, _) = RequestTracker::progress_reporter_callback(&tracker2.counters);

        assert_eq!(total1, 8); // 5 coverage + 3 docs
        assert_eq!(total2, 8);
        assert_eq!(completed1, 1);
        assert_eq!(completed2, 1);
    }
}
