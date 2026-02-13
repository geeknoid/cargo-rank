use crate::Result;
use fs4::fs_std::FileExt;
use ohno::IntoAppError;
use std::fs::{File, OpenOptions};
use std::path::Path;

/// Log target for `cache_lock`
const LOG_TARGET: &str = " collector";

/// Guard that releases the cache lock when dropped
#[derive(Debug)]
pub struct CacheLockGuard(File);

impl Drop for CacheLockGuard {
    fn drop(&mut self) {
        // Lock is automatically released when the file is closed
        // Log if unlock fails (shouldn't happen in normal operation)
        if let Err(e) = self.0.unlock() {
            log::warn!(target: LOG_TARGET, "Could not unlock cache: {e:#}");
        }
    }
}

/// Acquire a cache lock using advisory file locking
pub async fn acquire_cache_lock(cache_dir: &Path) -> Result<CacheLockGuard> {
    let lock_path = cache_dir.join("cache.lock");

    // Create or open the lock file
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .into_app_err_with(|| format!("opening cache lock file at '{}'", lock_path.display()))?;

    // Block until we can acquire the lock
    // This needs to run in a blocking task since it may block for an extended time
    let file = tokio::task::spawn_blocking(move || {
        file.lock_exclusive()
            .into_app_err_with(|| format!("acquiring exclusive lock on cache at '{}'", lock_path.display()))?;
        log::debug!(target: LOG_TARGET, "Acquired cache lock at '{}'", lock_path.display());
        Ok::<_, ohno::AppError>(file)
    })
    .await
    .into_app_err("lock task panicked")??;

    Ok(CacheLockGuard(file))
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    async fn test_acquire_lock_creates_lock_file() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let lock_path = temp_dir.path().join("cache.lock");

        assert!(!lock_path.exists());

        let guard = acquire_cache_lock(temp_dir.path()).await;
        assert!(guard.is_ok());
        assert!(lock_path.exists());

        drop(guard);
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    async fn test_lock_released_on_drop() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

        // Acquire and release
        let guard = acquire_cache_lock(temp_dir.path()).await.unwrap();
        drop(guard);

        // Should be able to re-acquire immediately after release
        let guard2 = acquire_cache_lock(temp_dir.path()).await;
        let _ = guard2.unwrap();
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    async fn test_acquire_lock_twice_sequentially() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");

        let guard1 = acquire_cache_lock(temp_dir.path()).await.unwrap();
        drop(guard1);

        let guard2 = acquire_cache_lock(temp_dir.path()).await.unwrap();
        drop(guard2);
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetFullPathNameW")]
    async fn test_acquire_lock_nonexistent_directory() {
        let path = Path::new("this_directory_does_not_exist_at_all_98765");
        let result = acquire_cache_lock(path).await;
        let _ = result.unwrap_err();
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    async fn test_lock_guard_debug() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let guard = acquire_cache_lock(temp_dir.path()).await.unwrap();
        let debug = format!("{guard:?}");
        assert!(debug.contains("CacheLockGuard"));
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    async fn test_exclusive_lock_blocks_concurrent_access() {
        use std::sync::Arc;
        use tokio::sync::Barrier;

        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let dir_path = temp_dir.path().to_path_buf();

        let barrier = Arc::new(Barrier::new(2));
        let counter = Arc::new(core::sync::atomic::AtomicU32::new(0));

        // Task 1: acquire lock, wait for task 2 to start, hold lock briefly
        let b1 = Arc::clone(&barrier);
        let c1 = Arc::clone(&counter);
        let d1 = dir_path.clone();
        let t1 = tokio::spawn(async move {
            let guard = acquire_cache_lock(&d1).await.unwrap();
            let _ = c1.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
            let _ = b1.wait().await;
            // Hold lock for a bit so task 2 must wait
            tokio::time::sleep(core::time::Duration::from_millis(50)).await;
            drop(guard);
        });

        // Task 2: wait until task 1 has the lock, then try to acquire
        let b2 = Arc::clone(&barrier);
        let c2 = Arc::clone(&counter);
        let t2 = tokio::spawn(async move {
            let _ = b2.wait().await;
            // Task 1 already holds the lock
            let guard = acquire_cache_lock(&dir_path).await.unwrap();
            // By the time we get here, task 1 should have incremented
            assert!(c2.load(core::sync::atomic::Ordering::SeqCst) >= 1);
            drop(guard);
        });

        t1.await.unwrap();
        t2.await.unwrap();
    }
}
