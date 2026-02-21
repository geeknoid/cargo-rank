//! Request tracking for monitoring outstanding HTTP requests.

use super::progress::Progress;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use owo_colors::OwoColorize;
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

/// Visual status of a tracked topic, controlling its display color in the
/// progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TopicStatus {
    /// Normal active state (default color).
    Active = 0,
    /// Blocked / waiting (blinks yellow).
    Blocked = 1,
    /// All requests completed (green).
    Done = 2,
}

/// Counter for a specific tracked topic.
#[derive(Debug, Default)]
struct RequestCounter {
    issued: AtomicU64,
    completed: AtomicU64,
    status: AtomicU8,
}

/// Tracks outstanding requests and updates progress reporting.
///
/// This is used to monitor requests to external services like GitHub, docs.rs,
/// and `codecov.io`, providing visibility into the query phase of crate analysis.
///
/// Requests are tracked by topic, with separate counters for different request types.
#[derive(Clone)]
pub struct RequestTracker {
    counters: Arc<[RequestCounter; 4]>,
    progress: Arc<dyn Progress>,
}

impl core::fmt::Debug for RequestTracker {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RequestTracker")
            .field("counters", &self.counters)
            .field("progress", &"<dyn Progress>")
            .finish()
    }
}

impl RequestTracker {
    /// Create a new request tracker with the given progress reporter.
    #[must_use]
    pub fn new(progress: &Arc<dyn Progress>) -> Self {
        let counters: Arc<[RequestCounter; 4]> = Arc::default();

        let counters_clone = Arc::clone(&counters);
        let use_colors = progress.use_colors();
        progress.set_determinate(Box::new(move || Self::progress_reporter_callback(&counters_clone, use_colors)));

        Self {
            counters,
            progress: Arc::clone(progress),
        }
    }

    /// Print a message line without disrupting the progress indicator.
    pub fn println(&self, msg: &str) {
        self.progress.println(msg);
    }

    /// Mark that multiple new requests have been issued for the given topic.
    pub fn add_requests(&self, topic: TrackedTopic, count: u64) {
        let counter = &self.counters[topic.index()];
        let _ = counter.issued.fetch_add(count, Ordering::Relaxed);
    }

    /// Mark that a request has completed for the given topic.
    ///
    /// Automatically sets the topic status to [`TopicStatus::Done`] when all
    /// issued requests have completed.
    pub fn complete_request(&self, topic: TrackedTopic) {
        let counter = &self.counters[topic.index()];
        let completed = counter.completed.fetch_add(1, Ordering::Relaxed) + 1;
        let issued = counter.issued.load(Ordering::Relaxed);
        if completed >= issued && issued > 0 {
            counter.status.store(TopicStatus::Done as u8, Ordering::Relaxed);
        }
    }

    /// Set the visual status of a topic, controlling its color in the progress bar.
    pub fn set_topic_status(&self, topic: TrackedTopic, status: TopicStatus) {
        self.counters[topic.index()].status.store(status as u8, Ordering::Relaxed);
    }

    /// Compute current progress state from counters.
    ///
    /// Returns (`total_length`, `current_position`, `message_string`).
    fn progress_reporter_callback(counters: &[RequestCounter; 4], use_colors: bool) -> (u64, u64, String) {
        // Toggle every 500ms for the blink effect on blocked topics
        let blink_on = use_colors && {
            let ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            (ms / 500).is_multiple_of(2)
        };

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

                let text = format!("{completed}/{issued} {}", topic.name());
                let status = counter.status.load(Ordering::Relaxed);

                let styled = if use_colors && status == TopicStatus::Done as u8 {
                    format!("{}", text.green())
                } else if status == TopicStatus::Blocked as u8 && blink_on {
                    format!("{}", text.yellow())
                } else {
                    text
                };

                parts.push(styled);
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

    #[derive(Debug)]
    struct NoOpProgress;

    impl Progress for NoOpProgress {
        fn set_phase(&self, _phase: &str) {}
        fn set_determinate(&self, _callback: Box<dyn Fn() -> (u64, u64, String) + Send + Sync + 'static>) {}
        fn set_indeterminate(&self, _callback: Box<dyn Fn() -> String + Send + Sync + 'static>) {}
        fn println(&self, _msg: &str) {}
        fn done(&self) {}
    }

    fn test_tracker() -> RequestTracker {
        RequestTracker::new(&(Arc::new(NoOpProgress) as Arc<dyn Progress>))
    }

    /// Strip ANSI escape sequences from a string for assertion comparisons.
    fn strip_ansi(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip until 'm' (end of ANSI escape)
                for esc in chars.by_ref() {
                    if esc == 'm' {
                        break;
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

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
        let tracker = test_tracker();
        tracker.add_requests(TrackedTopic::Coverage, 1);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);

        assert_eq!(total, 1);
        assert_eq!(completed, 0);
        assert_eq!(message, "0/1 coverage");
    }

    #[test]
    fn test_add_multiple_requests() {
        let tracker = test_tracker();
        tracker.add_requests(TrackedTopic::Docs, 5);
        tracker.add_requests(TrackedTopic::Repos, 3);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);

        assert_eq!(total, 8);
        assert_eq!(completed, 0);
        assert!(message.contains("0/5 docs"));
        assert!(message.contains("0/3 repos"));
    }

    #[test]
    fn test_complete_request() {
        let tracker = test_tracker();
        tracker.add_requests(TrackedTopic::Coverage, 3);
        tracker.complete_request(TrackedTopic::Coverage);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);

        assert_eq!(total, 3);
        assert_eq!(completed, 1);
        assert_eq!(message, "1/3 coverage");
    }

    #[test]
    fn test_complete_multiple_requests() {
        let tracker = test_tracker();
        tracker.add_requests(TrackedTopic::Codebase, 5);
        tracker.complete_request(TrackedTopic::Codebase);
        tracker.complete_request(TrackedTopic::Codebase);
        tracker.complete_request(TrackedTopic::Codebase);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);

        assert_eq!(total, 5);
        assert_eq!(completed, 3);
        assert_eq!(message, "3/5 codebase");
    }

    #[test]
    fn test_all_requests_completed() {
        let tracker = test_tracker();
        tracker.add_requests(TrackedTopic::Docs, 2);
        tracker.complete_request(TrackedTopic::Docs);
        tracker.complete_request(TrackedTopic::Docs);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);

        assert_eq!(total, 2);
        assert_eq!(completed, 2);
        assert_eq!(message, "2/2 docs");
    }

    #[test]
    fn test_completed_topic_colored_green() {
        let tracker = test_tracker();
        tracker.add_requests(TrackedTopic::Docs, 2);
        tracker.complete_request(TrackedTopic::Docs);
        tracker.complete_request(TrackedTopic::Docs);

        let (_, _, message) = RequestTracker::progress_reporter_callback(&tracker.counters, true);

        assert_eq!(strip_ansi(&message), "2/2 docs");
        // Green ANSI escape
        assert!(message.contains("\x1b[32m"));
    }

    #[test]
    fn test_blocked_topic_blinks_yellow() {
        let tracker = test_tracker();
        tracker.add_requests(TrackedTopic::Repos, 5);
        tracker.set_topic_status(TrackedTopic::Repos, TopicStatus::Blocked);

        // Sample across a full 1-second blink cycle to catch both phases
        let mut saw_yellow = false;
        let mut saw_plain = false;
        for _ in 0..12 {
            let (_, _, msg) = RequestTracker::progress_reporter_callback(&tracker.counters, true);
            assert_eq!(strip_ansi(&msg), "0/5 repos");
            if msg.contains("\x1b[33m") {
                saw_yellow = true;
            } else {
                saw_plain = true;
            }
            if saw_yellow && saw_plain {
                break;
            }
            std::thread::sleep(core::time::Duration::from_millis(100));
        }
        assert!(saw_yellow, "expected at least one blink phase to be yellow");
        assert!(saw_plain, "expected at least one blink phase to be plain");
    }

    #[test]
    fn test_multiple_topics_mixed_progress() {
        let tracker = test_tracker();
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

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);

        assert_eq!(total, 25);
        assert_eq!(completed, 9);
        assert!(message.contains("3/10 coverage"));
        assert!(message.contains("5/5 docs"));
        assert!(message.contains("1/3 repos"));
        assert!(message.contains("0/7 codebase"));
    }

    #[test]
    fn test_no_requests() {
        let tracker = test_tracker();

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);

        assert_eq!(total, 0);
        assert_eq!(completed, 0);
        assert_eq!(message, "No requests");
    }

    #[test]
    fn test_add_requests_with_zero_count() {
        let tracker = test_tracker();
        tracker.add_requests(TrackedTopic::Coverage, 0);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);

        assert_eq!(total, 0);
        assert_eq!(completed, 0);
        assert_eq!(message, "No requests");
    }

    #[test]
    fn test_message_format_order() {
        let tracker = test_tracker();
        // Add in reverse order to ensure output follows topic enum order
        tracker.add_requests(TrackedTopic::Codebase, 1);
        tracker.add_requests(TrackedTopic::Repos, 1);
        tracker.add_requests(TrackedTopic::Docs, 1);
        tracker.add_requests(TrackedTopic::Coverage, 1);

        let (_, _, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);

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
        let tracker1 = test_tracker();
        tracker1.add_requests(TrackedTopic::Coverage, 5);
        tracker1.complete_request(TrackedTopic::Coverage);

        let tracker2 = tracker1.clone();
        tracker2.add_requests(TrackedTopic::Docs, 3);

        // Both trackers should share the same counters
        let (total1, completed1, _) = RequestTracker::progress_reporter_callback(&tracker1.counters, false);
        let (total2, completed2, _) = RequestTracker::progress_reporter_callback(&tracker2.counters, false);

        assert_eq!(total1, 8); // 5 coverage + 3 docs
        assert_eq!(total2, 8);
        assert_eq!(completed1, 1);
        assert_eq!(completed2, 1);
    }

    #[test]
    fn test_println_delegates_to_progress() {
        use std::sync::Mutex;

        #[derive(Debug)]
        struct RecordingProgress {
            messages: Mutex<Vec<String>>,
        }

        impl Progress for RecordingProgress {
            fn set_phase(&self, _phase: &str) {}
            fn set_determinate(&self, _callback: Box<dyn Fn() -> (u64, u64, String) + Send + Sync + 'static>) {}
            fn set_indeterminate(&self, _callback: Box<dyn Fn() -> String + Send + Sync + 'static>) {}
            fn println(&self, msg: &str) {
                self.messages.lock().unwrap().push(msg.to_string());
            }
            fn done(&self) {}
        }

        let progress = Arc::new(RecordingProgress {
            messages: Mutex::new(Vec::new()),
        });
        let tracker = RequestTracker::new(&(Arc::clone(&progress) as Arc<dyn Progress>));

        tracker.println("hello");
        tracker.println("world");

        let messages = progress.messages.lock().unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0], "hello");
        assert_eq!(messages[1], "world");
        drop(messages);
    }

    #[test]
    fn test_debug_impl() {
        let tracker = test_tracker();
        let debug_str = format!("{tracker:?}");
        assert!(debug_str.contains("RequestTracker"));
        assert!(debug_str.contains("counters"));
        assert!(debug_str.contains("<dyn Progress>"));
    }

    #[test]
    fn test_tracked_topic_ordering() {
        assert!(TrackedTopic::Coverage < TrackedTopic::Docs);
        assert!(TrackedTopic::Docs < TrackedTopic::Repos);
        assert!(TrackedTopic::Repos < TrackedTopic::Codebase);
    }

    #[test]
    fn test_add_requests_large_count() {
        let tracker = test_tracker();
        tracker.add_requests(TrackedTopic::Coverage, 1_000_000);

        let (total, completed, message) = RequestTracker::progress_reporter_callback(&tracker.counters, false);
        assert_eq!(total, 1_000_000);
        assert_eq!(completed, 0);
        assert!(message.contains("0/1000000 coverage"));
    }
}
