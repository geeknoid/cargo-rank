use crate::facts::Progress;
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
const DETERMINATE_TEMPLATE_NO_COLOR: &str = "{prefix:>12} [{bar:25}] {msg}";
const INDETERMINATE_TEMPLATE: &str = "{prefix:>12.bold.cyan} [{spinner}] {msg}";
const INDETERMINATE_TEMPLATE_NO_COLOR: &str = "{prefix:>12} [{spinner}] {msg}";

struct DelayedProgressState {
    visible_after: Instant,
    visible: AtomicBool,
    is_indeterminate: AtomicBool,
    phase_start_time: Mutex<Instant>,
}

impl Debug for DelayedProgressState {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DelayedProgressState")
            .field("visible_after", &self.visible_after)
            .field("visible", &self.visible)
            .field("is_indeterminate", &self.is_indeterminate)
            .field("phase_start_time", &"<Instant>")
            .finish()
    }
}

/// A progress bar that delays showing itself until a threshold is reached.
#[derive(Clone)]
pub struct ProgressReporter {
    bar: ProgressBar,
    state: Arc<DelayedProgressState>,
    message_callback: Arc<Mutex<ProgressCallback>>,
    refresh_task: Arc<JoinHandle<()>>,
    use_colors: bool,
}

impl ProgressReporter {
    /// Create a new progress reporter.
    ///
    /// The progress bar will only become visible if operations continue beyond the delay threshold.
    /// When `use_colors` is false, progress bar chrome is rendered without ANSI styling.
    #[must_use]
    pub fn new(delay: Duration, use_colors: bool) -> Self {
        let bar = ProgressBar::hidden();
        bar.set_draw_target(ProgressDrawTarget::hidden());

        let state = Arc::new(DelayedProgressState {
            visible_after: Instant::now() + delay,
            visible: AtomicBool::new(false),
            is_indeterminate: AtomicBool::new(false),
            phase_start_time: Mutex::new(Instant::now()),
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
            use_colors,
        }
    }
}

impl Progress for ProgressReporter {
    /// Set the prefix label for the progress bar (e.g., "Preparing", "Collecting").
    fn set_phase(&self, phase: &str) {
        self.bar.set_prefix(phase.to_string());
        *self.state.phase_start_time.lock().expect("lock poisoned") = Instant::now();
    }

    /// Configure determinate progress reporting with a (total, current, message) callback.
    fn set_determinate(&self, callback: Box<dyn Fn() -> (u64, u64, String) + Send + Sync + 'static>) {
        *self.message_callback.lock().expect("lock poisoned") = callback;
        self.state.is_indeterminate.store(false, Ordering::Relaxed);
        self.bar.disable_steady_tick();
        self.bar.set_length(0);
        self.bar.set_position(0);
        let template = if self.use_colors { DETERMINATE_TEMPLATE } else { DETERMINATE_TEMPLATE_NO_COLOR };
        self.bar.set_style(
            ProgressStyle::default_bar()
                .template(template)
                .expect("could not create progress bar style")
                .progress_chars("=> "),
        );
    }

    /// Configure indeterminate progress reporting with a message-only callback.
    fn set_indeterminate(&self, callback: Box<dyn Fn() -> String + Send + Sync + 'static>) {
        *self.message_callback.lock().expect("lock poisoned") = Box::new(move || {
            let message = callback();
            (0, 0, message)
        });
        *self.state.phase_start_time.lock().expect("lock poisoned") = Instant::now();
        self.state.is_indeterminate.store(true, Ordering::Relaxed);
        self.bar.enable_steady_tick(Duration::from_millis(REFRESH_INTERVAL_MS));

        let template = if self.use_colors { INDETERMINATE_TEMPLATE } else { INDETERMINATE_TEMPLATE_NO_COLOR };
        self.bar.set_style(
            ProgressStyle::default_spinner()
                .template(template)
                .expect("could not create progress bar style")
                .tick_strings(&[
                    ">                        ", // 1–4 chars padded with spaces to total 25 characters
                    "=>                       ",
                    "==>                      ",
                    "===>                     ",
                    " ===>                    ",
                    "  ===>                   ",
                    "   ===>                  ",
                    "    ===>                 ",
                    "     ===>                ",
                    "      ===>               ",
                    "       ===>              ",
                    "        ===>             ",
                    "         ===>            ",
                    "          ===>           ",
                    "           ===>          ",
                    "            ===>         ",
                    "             ===>        ",
                    "              ===>       ",
                    "               ===>      ",
                    "                ===>     ",
                    "                 ===>    ",
                    "                  ===>   ",
                    "                   ===>  ",
                    "                    ===> ",
                    "                     ===>",
                    "                      ===",
                    "                       ==",
                    "                        =",
                    "                         ",
                    "                        <",
                    "                       <=",
                    "                      <==",
                    "                     <===",
                    "                    <=== ",
                    "                   <===  ",
                    "                  <===   ",
                    "                 <===    ",
                    "                <===     ",
                    "               <===      ",
                    "              <===       ",
                    "             <===        ",
                    "            <===         ",
                    "           <===          ",
                    "          <===           ",
                    "         <===            ",
                    "        <===             ",
                    "       <===              ",
                    "      <===               ",
                    "     <===                ",
                    "    <===                 ",
                    "   <===                  ",
                    "  <===                   ",
                    " <===                    ",
                    "<===                     ",
                    "===                      ",
                    "==                       ",
                    "=                        ",
                    "                         ",
                ]),
        );
    }

    /// Print a message line without disrupting the progress indicator.
    fn println(&self, msg: &str) {
        self.bar.suspend(|| eprintln!("{msg}"));
    }

    /// Finish and clear the progress indicator.
    fn done(&self) {
        self.refresh_task.abort();
        if self.state.visible.load(Ordering::Relaxed) {
            self.bar.finish_and_clear();
        }
    }

    fn use_colors(&self) -> bool {
        self.use_colors
    }
}

impl Debug for ProgressReporter {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ProgressReporter")
            .field("bar", &self.bar)
            .field("state", &self.state)
            .field("message_callback", &"<callback>")
            .field("refresh_task", &"<task>")
            .field("use_colors", &self.use_colors)
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
            let (length, position, mut message) = {
                let callback_guard = callback.lock().expect("lock poisoned");
                callback_guard()
            };

            // In indeterminate mode, prepend elapsed seconds to the message
            if state.is_indeterminate.load(Ordering::Relaxed) {
                let elapsed_secs = {
                    let start_time = state.phase_start_time.lock().expect("lock poisoned");
                    start_time.elapsed().as_secs()
                };
                message = format!("{elapsed_secs}s: {message}");
            }

            if length > 0 {
                bar.set_length(length);
                bar.set_position(position);
            }
            bar.set_message(message);
        }
    }
}
