use super::provider::LOG_TARGET;
use crate::Result;
use chrono::{DateTime, Utc};
use core::time::Duration;
use ohno::{IntoAppError, bail};
use std::fs;
use std::path::Path;
use tokio::process::Command;
use url::Url;

const GIT_TIMEOUT: Duration = Duration::from_mins(5);

/// Convert a path to a UTF-8 string, returning an error if the path contains invalid UTF-8.
fn path_str(path: &Path) -> Result<&str> {
    path.to_str().into_app_err("invalid UTF-8 in repository path")
}

/// Result of a repository sync operation.
pub enum RepoStatus {
    /// Repository was successfully cloned or updated.
    Ok,
    /// Repository does not exist on the remote.
    NotFound,
}

/// Clone or update a git repository
pub async fn get_repo(repo_path: &Path, repo_url: &Url) -> Result<RepoStatus> {
    let start_time = std::time::Instant::now();

    let status = get_repo_core(repo_path, repo_url).await?;

    if matches!(status, RepoStatus::Ok) {
        log::debug!(target: LOG_TARGET, "Successfully prepared cached repository from '{repo_url}' in {:.3}s", start_time.elapsed().as_secs_f64());
    }

    Ok(status)
}

async fn get_repo_core(repo_path: &Path, repo_url: &Url) -> Result<RepoStatus> {
    let path_str = path_str(repo_path)?;

    if !repo_path.exists() {
        if let Some(parent) = repo_path.parent() {
            fs::create_dir_all(parent).into_app_err_with(|| format!("creating directory '{}'", parent.display()))?;
        }

        return clone_repo(path_str, repo_url).await;
    }

    // Verify it's a valid git repository before attempting update
    if !repo_path.join(".git").exists() {
        log::warn!(target: LOG_TARGET, "Cached repository path '{path_str}' exists but .git directory missing, re-cloning");
        fs::remove_dir_all(repo_path)
            .into_app_err_with(|| format!("removing potentially corrupt cached repository '{path_str}'"))?;
        return clone_repo(path_str, repo_url).await;
    }

    log::info!(target: LOG_TARGET, "Syncing repository '{repo_url}'");

    // First, try to fetch new commits
    // --filter=blob:none downloads only commit/tree objects, not file contents
    // --prune removes refs that no longer exist on remote
    // --force allows updating refs even if they're not fast-forward
    let output = run_git_with_timeout(&["-C", path_str, "fetch", "origin", "--filter=blob:none", "--prune", "--force"]).await?;

    if !output.status.success() {
        // Fetch failed - repository might be corrupted, try re-clone
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::warn!(target: LOG_TARGET, "Git fetch failed ({}), removing and re-cloning", stderr.trim());
        fs::remove_dir_all(path_str).into_app_err_with(|| format!("removing stale cached repository '{path_str}'"))?;
        return clone_repo(path_str, repo_url).await;
    }

    // Reset to match remote HEAD (discard any local changes)
    let output = run_git_with_timeout(&["-C", path_str, "reset", "--hard", "origin/HEAD"]).await?;
    check_git_output(&output, "git reset")?;
    Ok(RepoStatus::Ok)
}

/// Check whether git stderr indicates the repository was not found on the remote.
fn is_repo_not_found(stderr: &str) -> bool {
    let stderr_lower = stderr.to_lowercase();
    stderr_lower.contains("not found") || stderr_lower.contains("does not exist")
}

async fn clone_repo(repo_path: &str, repo_url: &Url) -> Result<RepoStatus> {
    log::info!(target: LOG_TARGET, "Syncing repository '{repo_url}'");
    // --filter=blob:none creates a partial clone with full history but no blob contents
    let output = run_git_with_timeout(&[
        "clone",
        "--filter=blob:none",
        "--single-branch",
        "--no-tags",
        repo_url.as_str(),
        repo_path,
    ])
    .await?;

    if output.status.success() {
        return Ok(RepoStatus::Ok);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Clean up any partial clone directory left behind
    let path = Path::new(repo_path);
    if path.exists() {
        let _ = fs::remove_dir_all(path);
    }

    if is_repo_not_found(&stderr) {
        log::debug!(target: LOG_TARGET, "Repository '{repo_url}' not found on remote");
        return Ok(RepoStatus::NotFound);
    }

    bail!("git clone failed: {stderr}");
}

fn check_git_output(output: &std::process::Output, operation: &str) -> Result<()> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{operation} failed: {stderr}");
    }
    Ok(())
}

/// Count unique contributors in the repository
pub async fn count_contributors(repo_path: &Path) -> Result<u64> {
    let path_str = path_str(repo_path)?;
    // -s = summary (count only), -n = sort by count, -e = show emails
    // --all ensures we count contributors from all fetched refs, not just HEAD
    let output = run_git_with_timeout(&["-C", path_str, "shortlog", "-sne", "--all"]).await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git shortlog failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().count() as u64)
}

/// Commit statistics gathered from a single git log invocation.
pub struct CommitStats {
    /// Total number of commits.
    pub commit_count: u64,
    /// Timestamp of the first (oldest) commit.
    pub first_commit_at: DateTime<Utc>,
    /// Timestamp of the most recent commit.
    pub last_commit_at: DateTime<Utc>,
    /// Number of commits within each requested time window, in the same order as the input.
    pub commits_per_window: Vec<u64>,
}

/// Gather commit statistics from a single `git log` invocation.
///
/// Returns total count, first/last commit timestamps, and per-window commit counts
/// for each entry in `day_windows`. Uses Unix timestamps for efficient comparison.
pub async fn get_commit_stats(repo_path: &Path, day_windows: &[i64]) -> Result<CommitStats> {
    let path_str = path_str(repo_path)?;

    // %at = author date as Unix timestamp
    let output = run_git_with_timeout(&["-C", path_str, "log", "--format=%at"]).await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git log failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let now = Utc::now().timestamp();

    let mut commit_count: u64 = 0;
    let mut first_timestamp: Option<i64> = None;
    let mut last_timestamp: Option<i64> = None;
    let mut window_counts = vec![0u64; day_windows.len()];
    let window_thresholds: Vec<i64> = day_windows.iter().map(|days| now - days * 86400).collect();

    for line in stdout.lines() {
        let ts: i64 = match line.trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };

        commit_count += 1;

        // git log outputs newest first, so first parsed is last_timestamp, last parsed is first_timestamp
        if last_timestamp.is_none() {
            last_timestamp = Some(ts);
        }
        first_timestamp = Some(ts);

        for (i, threshold) in window_thresholds.iter().enumerate() {
            if ts >= *threshold {
                window_counts[i] += 1;
            }
        }
    }

    let first_commit_at = first_timestamp
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .unwrap_or(DateTime::UNIX_EPOCH);

    let last_commit_at = last_timestamp
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .unwrap_or(DateTime::UNIX_EPOCH);

    Ok(CommitStats {
        commit_count,
        first_commit_at,
        last_commit_at,
        commits_per_window: window_counts,
    })
}

async fn run_git_with_timeout(args: &[&str]) -> Result<std::process::Output> {
    let child = Command::new("git")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .into_app_err("spawning git command")?;

    match tokio::time::timeout(GIT_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(e).into_app_err_with(|| format!("running 'git {}'", args.join(" "))),
        Err(_) => {
            bail!("'git {}' timed out after {} seconds", args.join(" "), GIT_TIMEOUT.as_secs());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{ExitStatus, Output};

    #[test]
    fn test_check_git_output_success() {
        #[cfg(unix)]
        let status = {
            use std::os::unix::process::ExitStatusExt;
            ExitStatus::from_raw(0)
        };

        #[cfg(windows)]
        let status = {
            use std::os::windows::process::ExitStatusExt;
            ExitStatus::from_raw(0)
        };

        let output = Output {
            status,
            stdout: vec![],
            stderr: vec![],
        };

        check_git_output(&output, "test operation").unwrap();
    }

    #[test]
    fn test_check_git_output_failure() {
        #[cfg(unix)]
        let status = {
            use std::os::unix::process::ExitStatusExt;
            ExitStatus::from_raw(256) // Exit code 1
        };

        #[cfg(windows)]
        let status = {
            use std::os::windows::process::ExitStatusExt;
            ExitStatus::from_raw(1)
        };

        let output = Output {
            status,
            stdout: vec![],
            stderr: b"error: failed to do something".to_vec(),
        };

        let result = check_git_output(&output, "test operation");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test operation failed"));
    }

    #[test]
    fn test_is_repo_not_found_positive() {
        assert!(is_repo_not_found("Repository not found"));
        assert!(is_repo_not_found("ERROR: Repository not found."));
        assert!(is_repo_not_found("remote: Repository does not exist"));
        assert!(is_repo_not_found("fatal: repository 'https://...' does not exist"));
    }

    #[test]
    fn test_is_repo_not_found_negative() {
        assert!(!is_repo_not_found("fatal: unable to access"));
        assert!(!is_repo_not_found("Permission denied"));
        assert!(!is_repo_not_found(""));
    }

    #[test]
    fn test_is_repo_not_found_case_insensitive() {
        assert!(is_repo_not_found("NOT FOUND"));
        assert!(is_repo_not_found("DOES NOT EXIST"));
        assert!(is_repo_not_found("Not Found"));
    }

    #[test]
    fn test_path_str_valid_utf8() {
        let path = Path::new("/tmp/test");
        assert_eq!(path_str(path).unwrap(), "/tmp/test");
    }

    #[test]
    fn test_check_git_output_with_stderr() {
        #[cfg(unix)]
        let status = {
            use std::os::unix::process::ExitStatusExt;
            ExitStatus::from_raw(256)
        };

        #[cfg(windows)]
        let status = {
            use std::os::windows::process::ExitStatusExt;
            ExitStatus::from_raw(1)
        };

        let stderr_msg = b"fatal: not a git repository";
        let output = Output {
            status,
            stdout: vec![],
            stderr: stderr_msg.to_vec(),
        };

        let result = check_git_output(&output, "git status");
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains("git status failed"));
        assert!(error_msg.contains("not a git repository"));
    }

    /// Create a temp git repository with a few commits for testing.
    /// Returns the tempdir (must be kept alive) and the repo path.
    fn create_test_repo() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let repo_path = tmp.path().join("test-repo");
        fs::create_dir_all(&repo_path).expect("create repo dir");

        // Initialize a repo and make commits
        let init = |args: &[&str]| {
            let _ = std::process::Command::new("git")
                .args(args)
                .current_dir(&repo_path)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .expect("run git command");
        };

        init(&["init"]);
        init(&["config", "user.email", "test@test.com"]);
        init(&["config", "user.name", "Test User"]);

        // Create two commits so we have meaningful stats
        fs::write(repo_path.join("file1.txt"), "hello").expect("write file1");
        init(&["add", "."]);
        init(&["commit", "-m", "first commit"]);

        fs::write(repo_path.join("file2.txt"), "world").expect("write file2");
        init(&["add", "."]);
        init(&["commit", "-m", "second commit"]);

        (tmp, repo_path)
    }

    /// Helper to set up a bare repo with one commit and return the temp dir + bare path.
    fn create_bare_repo_with_commit() -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let bare_path = tmp.path().join("bare.git");

        let run = |args: &[&str], dir: &Path| {
            let _ = std::process::Command::new("git")
                .args(args)
                .current_dir(dir)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .output()
                .expect("run git command");
        };

        fs::create_dir_all(&bare_path).expect("create bare dir");
        run(&["init", "--bare"], &bare_path);

        let work_path = tmp.path().join("work");
        let _ = std::process::Command::new("git")
            .args(["clone", bare_path.to_str().unwrap(), work_path.to_str().unwrap()])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .unwrap();
        run(&["config", "user.email", "test@test.com"], &work_path);
        run(&["config", "user.name", "Test User"], &work_path);
        fs::write(work_path.join("file.txt"), "content").expect("write file");
        run(&["add", "."], &work_path);
        run(&["commit", "-m", "initial"], &work_path);
        run(&["push"], &work_path);

        (tmp, bare_path)
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_run_git_with_timeout_success() {
        let output = run_git_with_timeout(&["--version"]).await.unwrap();
        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("git version"));
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_run_git_with_timeout_failure() {
        // Run git log in a directory that is not a repo
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().to_str().unwrap();
        let output = run_git_with_timeout(&["-C", path, "log"]).await.unwrap();
        assert!(!output.status.success());
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_count_contributors() {
        let (_tmp, repo_path) = create_test_repo();
        let count = count_contributors(&repo_path).await.unwrap();
        assert_eq!(count, 1); // Single test user
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_count_contributors_failure() {
        let tmp = tempfile::tempdir().unwrap();
        // Not a git repo - shortlog should fail
        let result = count_contributors(tmp.path()).await;
        let _ = result.unwrap_err();
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_get_commit_stats_basic() {
        let (_tmp, repo_path) = create_test_repo();
        let stats = get_commit_stats(&repo_path, &[30, 365]).await.unwrap();
        assert_eq!(stats.commit_count, 2);
        assert!(stats.first_commit_at <= stats.last_commit_at);
        assert_eq!(stats.commits_per_window.len(), 2);
        // Both commits were just made, so they should be within both windows
        assert_eq!(stats.commits_per_window[0], 2); // last 30 days
        assert_eq!(stats.commits_per_window[1], 2); // last 365 days
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_get_commit_stats_empty_windows() {
        let (_tmp, repo_path) = create_test_repo();
        let stats = get_commit_stats(&repo_path, &[]).await.unwrap();
        assert_eq!(stats.commit_count, 2);
        assert!(stats.commits_per_window.is_empty());
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_get_commit_stats_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let result = get_commit_stats(tmp.path(), &[30]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_get_repo_clone_from_bare() {
        let (tmp, bare_path) = create_bare_repo_with_commit();

        // Clone into a new path via get_repo
        let clone_path = tmp.path().join("clone");
        let bare_url = Url::from_file_path(&bare_path).unwrap();
        let status = get_repo(&clone_path, &bare_url).await.unwrap();
        assert!(matches!(status, RepoStatus::Ok));
        assert!(clone_path.join(".git").exists());

        // Call get_repo again to exercise the fetch+reset path
        let status = get_repo(&clone_path, &bare_url).await.unwrap();
        assert!(matches!(status, RepoStatus::Ok));
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_get_repo_reclones_when_git_dir_missing() {
        let (tmp, bare_path) = create_bare_repo_with_commit();

        // Clone via get_repo
        let clone_path = tmp.path().join("clone");
        let bare_url = Url::from_file_path(&bare_path).unwrap();
        let status = get_repo(&clone_path, &bare_url).await.unwrap();
        assert!(matches!(status, RepoStatus::Ok));

        // Remove .git to simulate corruption
        fs::remove_dir_all(clone_path.join(".git")).unwrap();

        // get_repo should detect missing .git and re-clone
        let status = get_repo(&clone_path, &bare_url).await.unwrap();
        assert!(matches!(status, RepoStatus::Ok));
        assert!(clone_path.join(".git").exists());
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot run external commands")]
    async fn test_get_repo_nonexistent_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let clone_path = tmp.path().join("clone");
        let bad_url = Url::from_file_path(tmp.path().join("nonexistent.git")).unwrap();
        // Cloning a non-existent local path either returns NotFound or an error,
        // depending on the exact git error message. Either way, it should not succeed.
        if let Ok(status) = get_repo(&clone_path, &bad_url).await {
            assert!(matches!(status, RepoStatus::NotFound));
        }
        // Also acceptable — git error message didn't match not-found patterns
    }
}
