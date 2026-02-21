use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::{Notify, Semaphore};

/// Limits concurrency and supports temporary pausing of work dispatch.
///
/// Wrap in an `Arc` via [`Throttler::new`], then call [`Throttler::acquire`] before
/// each unit of work. At most `max_concurrent` tasks will run simultaneously.
/// Any task can call [`Throttler::pause_for`] to temporarily halt new work dispatch
/// (e.g. in response to downstream backpressure).
///
/// When multiple tasks call [`Throttler::pause_for`] concurrently, the longest
/// pause wins — shorter pauses are ignored if a longer one is already active.
#[derive(Debug)]
pub struct Throttler {
    semaphore: Arc<Semaphore>,
    paused: AtomicBool,
    resume: Notify,
    /// Tracks when the current pause should expire. Used to ensure the longest
    /// pause wins when multiple `pause_for` calls overlap.
    resume_at: std::sync::Mutex<Option<Instant>>,
}

impl Throttler {
    /// Create a new throttler that allows at most `max_concurrent` tasks at a time.
    pub fn new(max_concurrent: usize) -> Arc<Self> {
        Arc::new(Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            paused: AtomicBool::new(false),
            resume: Notify::new(),
            resume_at: std::sync::Mutex::new(None),
        })
    }

    /// Wait until unpaused, then acquire a concurrency slot.
    ///
    /// The returned permit must be held for the duration of the work. When it
    /// is dropped, the slot becomes available for another task.
    pub async fn acquire(&self) -> tokio::sync::OwnedSemaphorePermit {
        loop {
            if self.paused.load(Ordering::Acquire) {
                self.resume.notified().await;
                continue;
            }

            return Arc::clone(&self.semaphore)
                .acquire_owned()
                .await
                .expect("semaphore is never closed");
        }
    }

    /// Returns whether the throttler is currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Acquire)
    }

    /// Minimum extension required for a new pause to override an active one.
    /// Prevents near-simultaneous callers (e.g. concurrent tasks that all
    /// discovered the same rate-limit reset time) from each "winning" the pause
    /// due to tiny `Instant::now()` drift between calls.
    const MIN_PAUSE_EXTENSION: Duration = Duration::from_secs(1);

    /// Pause dispatching for `duration`, then automatically resume.
    ///
    /// Tasks already running are not interrupted. Tasks waiting in [`acquire`](Self::acquire)
    /// will remain parked until the duration elapses. If a pause with a similar
    /// or longer duration is already active, this call is a no-op and returns `false`.
    /// Returns `true` only when a new pause is actually established.
    pub fn pause_for(self: &Arc<Self>, duration: Duration) -> bool {
        let new_resume_at = Instant::now() + duration;

        {
            let mut guard = self.resume_at.lock().expect("lock not poisoned");
            if guard.is_some_and(|existing| existing + Self::MIN_PAUSE_EXTENSION >= new_resume_at) {
                return false; // an equivalent or longer pause is already active
            }
            *guard = Some(new_resume_at);
        }

        self.paused.store(true, Ordering::Release);
        let this = Arc::clone(self);
        drop(tokio::spawn(async move {
            tokio::time::sleep(duration).await;

            let should_resume = {
                let mut guard = this.resume_at.lock().expect("lock not poisoned");
                if guard.is_some_and(|t| Instant::now() >= t) {
                    *guard = None;
                    true
                } else {
                    false // a longer pause was scheduled after us
                }
            };

            if should_resume {
                this.paused.store(false, Ordering::Release);
                this.resume.notify_waiters();
            }
        }));

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::AtomicUsize;

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort on Windows")]
    async fn limits_concurrency() {
        let throttler = Throttler::new(2);
        let active = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));

        let tasks: Vec<_> = (0..10)
            .map(|_| {
                let throttler = Arc::clone(&throttler);
                let active = Arc::clone(&active);
                let max_seen = Arc::clone(&max_seen);
                tokio::spawn(async move {
                    let _permit = throttler.acquire().await;
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                    _ = max_seen.fetch_max(current, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    _ = active.fetch_sub(1, Ordering::SeqCst);
                })
            })
            .collect();

        _ = futures_util::future::join_all(tasks).await;

        assert!(max_seen.load(Ordering::SeqCst) <= 2);
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort on Windows")]
    async fn pause_blocks_new_work() {
        let throttler = Throttler::new(5);

        // Pause for 200ms
        let _ = throttler.pause_for(Duration::from_millis(200));

        let start = tokio::time::Instant::now();
        let _permit = throttler.acquire().await;
        let elapsed = start.elapsed();

        // Should have waited at least ~200ms
        assert!(elapsed >= Duration::from_millis(150));
    }
}
