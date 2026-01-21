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
            log::warn!(target: LOG_TARGET, "Failed to unlock cache: {e}");
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
        .into_app_err_with(|| format!("Failed to open cache lock file at '{}'", lock_path.display()))?;

    // Block until we can acquire the lock
    // This needs to run in a blocking task since it may block for an extended time
    let file = tokio::task::spawn_blocking(move || {
        file.lock_exclusive()
            .into_app_err_with(|| format!("Failed to acquire exclusive lock on cache at '{}'", lock_path.display()))?;
        log::debug!(target: LOG_TARGET, "Acquired cache lock at '{}'", lock_path.display());
        Ok::<_, ohno::AppError>(file)
    })
    .await
    .into_app_err("Lock task panicked")??;

    Ok(CacheLockGuard(file))
}
