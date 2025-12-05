use core::fmt::{Debug, Formatter};
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::task::JoinHandle;

type ProgressCallback = Box<dyn Fn() -> (u64, u64, String) + Send + Sync>;

/// Refresh rate for progress updates (10 Hz).
const REFRESH_INTERVAL_MS: u64 = 100;

const DETERMINATE_TEMPLATE: &str = "{prefix:>12.bold.cyan} [{bar:25}] {msg}";
const INDETERMINATE_TEMPLATE: &str = "  {prefix:>8.bold.cyan} [{spinner}] {msg}";

#[derive(Debug)]
struct DelayedProgressState {
    visible_after: Instant,
    visible: AtomicBool,
    is_indeterminate: AtomicBool,
}

/// A progress bar that delays showing itself until a threshold is reached.
#[derive(Clone)]
pub struct ProgressReporter {
    bar: ProgressBar,
    state: Arc<DelayedProgressState>,
    message_callback: Arc<Mutex<ProgressCallback>>,
    refresh_task: Arc<JoinHandle<()>>,
}

impl ProgressReporter {
    /// Create a new progress reporter.
    ///
    /// The progress bar will only become visible if operations continue beyond the delay threshold.
    #[must_use]
    pub fn new(delay: Duration) -> Self {
        let bar = ProgressBar::hidden();
        bar.set_draw_target(ProgressDrawTarget::hidden());

        let state = Arc::new(DelayedProgressState {
            visible_after: Instant::now() + delay,
            visible: AtomicBool::new(false),
            is_indeterminate: AtomicBool::new(false),
        });

        let message_callback = Arc::new(Mutex::new(Box::new(|| (0u64, 0u64, String::new())) as ProgressCallback));

        Self {
            refresh_task: Arc::new(tokio::spawn(refresh_task(
                bar.clone(),
                Arc::clone(&state),
                Arc::clone(&message_callback),
            ))),
            bar,
            state,
            message_callback,
        }
    }

    /// Set the prefix label for the progress bar (e.g., "Preparing", "Collecting").
    pub fn set_phase(&self, prefix: &str) {
        self.bar.set_prefix(prefix.to_string());
    }

    /// Configure determinate progress reporting with a (total, current, message) callback.
    pub fn configure_determinate(&self, callback: impl Fn() -> (u64, u64, String) + Send + Sync + 'static) {
        *self.message_callback.lock().expect("lock poisoned") = Box::new(callback);
        self.state.is_indeterminate.store(false, Ordering::Relaxed);
        self.bar.disable_steady_tick();
        self.bar.set_length(0);
        self.bar.set_position(0);
        self.bar.set_style(
            ProgressStyle::default_bar()
                .template(DETERMINATE_TEMPLATE)
                .expect("Could not create progress bar style")
                .progress_chars("=> "),
        );
    }

    /// Configure indeterminate progress reporting with a message-only callback.
    pub fn configure_indeterminate(&self, callback: impl Fn() -> String + Send + Sync + 'static) {
        *self.message_callback.lock().expect("lock poisoned") = Box::new(move || {
            let message = callback();
            (0, 0, message)
        });
        self.state.is_indeterminate.store(true, Ordering::Relaxed);
        self.bar.enable_steady_tick(Duration::from_millis(REFRESH_INTERVAL_MS));

        self.bar.set_style(
            ProgressStyle::default_spinner()
                .template(INDETERMINATE_TEMPLATE)
                .expect("Could not create progress bar style")
                .tick_strings(&[
                    "=                        ", // 12 spaces, char, 12 spaces = 25 total
                    "==                       ",
                    "===                      ",
                    " ===                     ",
                    "  ===                    ",
                    "   ===                   ",
                    "    ===                  ",
                    "     ===                 ",
                    "      ===                ",
                    "       ===               ",
                    "        ===              ",
                    "         ===             ",
                    "          ===            ",
                    "           ===           ",
                    "            ===          ",
                    "             ===         ",
                    "              ===        ",
                    "               ===       ",
                    "                ===      ",
                    "                 ===     ",
                    "                  ===    ",
                    "                   ===   ",
                    "                    ===  ",
                    "                     === ",
                    "                      ===",
                    "                       ==",
                    "                        =",
                    "                       ==",
                    "                      ===",
                    "                     === ",
                    "                    ===  ",
                    "                   ===   ",
                    "                  ===    ",
                    "                 ===     ",
                    "                ===      ",
                    "               ===       ",
                    "              ===        ",
                    "             ===         ",
                    "            ===          ",
                    "           ===           ",
                    "          ===            ",
                    "         ===             ",
                    "        ===              ",
                    "       ===               ",
                    "      ===                ",
                    "     ===                 ",
                    "    ===                  ",
                    "   ===                   ",
                    "  ===                    ",
                    " ===                     ",
                    "===                      ",
                    "==                       ",
                ]),
        );
    }

    /// Finish and clear the progress indicator.
    pub fn finish_and_clear(&self) {
        self.refresh_task.abort();
        if self.state.visible.load(Ordering::Relaxed) {
            self.bar.finish_and_clear();
        }
    }
}

impl Debug for ProgressReporter {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ProgressReporter")
            .field("bar", &self.bar)
            .field("state", &self.state)
            .field("message_callback", &"<callback>")
            .field("refresh_task", &"<task>")
            .finish()
    }
}

/// Background refresh task that periodically updates the progress bar.
async fn refresh_task(bar: ProgressBar, state: Arc<DelayedProgressState>, callback: Arc<Mutex<ProgressCallback>>) {
    let mut interval = tokio::time::interval(Duration::from_millis(REFRESH_INTERVAL_MS));
    #[expect(clippy::infinite_loop, reason = "task runs until aborted")]
    loop {
        let _ = interval.tick().await;

        if !state.visible.load(Ordering::Relaxed) && Instant::now() >= state.visible_after {
            state.visible.store(true, Ordering::Relaxed);
            bar.set_draw_target(ProgressDrawTarget::stderr_with_hz(10));
        }

        if state.visible.load(Ordering::Relaxed) {
            let (length, position, message) = {
                let callback_guard = callback.lock().expect("lock poisoned");
                callback_guard()
            };

            if length > 0 {
                bar.set_length(length);
                bar.set_position(position);
            }
            bar.set_message(message);
        }
    }
}
