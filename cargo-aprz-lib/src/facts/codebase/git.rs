use super::provider::LOG_TARGET;
use crate::Result;
use core::time::Duration;
use ohno::{IntoAppError, bail};
use std::fs;
use std::path::Path;
use tokio::process::Command;
use url::Url;

const GIT_TIMEOUT: Duration = Duration::from_mins(5);

/// Clone or update a git repository
pub async fn get_repo(repo_path: &Path, repo_url: &Url) -> Result<()> {
    let start_time = std::time::Instant::now();

    get_repo_core(repo_path, repo_url).await?;

    log::debug!(target: LOG_TARGET, "Successfully prepared cached repository from '{repo_url}' in {:.3}s", start_time.elapsed().as_secs_f64());
    Ok(())
}

async fn get_repo_core(repo_path: &Path, repo_url: &Url) -> Result<()> {
    let path_str = repo_path.to_str().into_app_err("invalid UTF-8 in repository path")?;

    if !repo_path.exists() {
        if let Some(parent) = repo_path.parent() {
            fs::create_dir_all(parent).into_app_err_with(|| format!("could not create directory '{}'", parent.display()))?;
        }

        return clone_repo(path_str, repo_url).await;
    }

    // Verify it's a valid git repository before attempting update
    if !repo_path.join(".git").exists() {
        log::warn!(target: LOG_TARGET, "Cached repository path '{path_str}' exists but .git directory missing, re-cloning");
        fs::remove_dir_all(repo_path)
            .into_app_err_with(|| format!("could not remove potentially corrupt cached repository '{path_str}'"))?;
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
        fs::remove_dir_all(path_str).into_app_err_with(|| format!("could not remove stale cached repository '{path_str}'"))?;
        return clone_repo(path_str, repo_url).await;
    }

    // Reset to match remote HEAD (discard any local changes)
    let output = run_git_with_timeout(&["-C", path_str, "reset", "--hard", "origin/HEAD"]).await?;
    check_git_output(&output, "git reset")
}

async fn clone_repo(repo_path: &str, repo_url: &Url) -> Result<()> {
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
    check_git_output(&output, "git clone")
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
    let path_str = repo_path.to_str().into_app_err("invalid UTF-8 in repository path")?;

    // Get all author emails from git log across all refs
    // %ae = author email (respecting .mailmap)
    // --all ensures we count contributors from all fetched refs, not just HEAD
    let output = run_git_with_timeout(&["-C", path_str, "log", "--all", "--format=%ae"]).await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git log failed: {stderr}");
    }

    // Parse output and count unique emails
    let stdout = String::from_utf8_lossy(&output.stdout);
    let unique_contributors: std::collections::HashSet<&str> = stdout.lines().collect();

    Ok(unique_contributors.len() as u64)
}

async fn run_git_with_timeout(args: &[&str]) -> Result<std::process::Output> {
    let child = Command::new("git")
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .into_app_err("could not spawn git command")?;

    match tokio::time::timeout(GIT_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(e)) => Err(e).into_app_err_with(|| format!("'git {}' failed to run", args.join(" "))),
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
}
