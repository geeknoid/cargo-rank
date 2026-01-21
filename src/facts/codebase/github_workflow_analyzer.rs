//! Detector for CI tool usage in GitHub Actions CI workflows.

use super::provider::LOG_TARGET;
use crate::Result;
use ohno::IntoAppError;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::Path;

#[derive(Debug, Default, Clone)]
pub struct GitHubWorkflowInfo {
    pub workflows_detected: bool,
    pub clippy_detected: bool,
    pub miri_detected: bool,
}

/// Detect if Miri and Clippy are mentioned in GitHub Actions CI
pub fn sniff_github_workflows(repo_path: impl AsRef<Path>) -> Result<GitHubWorkflowInfo> {
    const MAX_WORKFLOW_FILES: usize = 100;

    let mut usage = GitHubWorkflowInfo::default();

    let workflows_dir = repo_path.as_ref().join(".github").join("workflows");
    if !workflows_dir.exists() {
        return Ok(usage);
    }

    usage.workflows_detected = true;

    let mut file_count = 0;

    for entry_result in walkdir::WalkDir::new(&workflows_dir).follow_links(false) {
        let entry = entry_result.into_app_err("could not walk workflows directory")?;

        // Skip directories
        if entry.file_type().is_dir() {
            continue;
        }

        // Check for YAML extension
        let is_yaml = entry
            .path()
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext == "yml" || ext == "yaml");

        if !is_yaml {
            continue;
        }

        file_count += 1;
        if file_count > MAX_WORKFLOW_FILES {
            log::warn!(target: LOG_TARGET, "Workflow file count limit ({MAX_WORKFLOW_FILES}) exceeded, stopping scan");
            break;
        }

        let file =
            fs::File::open(entry.path()).into_app_err_with(|| format!("could not open workflow file '{}'", entry.path().display()))?;
        let reader = BufReader::new(file);

        for line in reader.lines().map_while(Result::ok) {
            if !usage.miri_detected && line.to_lowercase().contains("miri") {
                usage.miri_detected = true;
            }

            if !usage.clippy_detected && line.to_lowercase().contains("clippy") {
                usage.clippy_detected = true;
            }

            if usage.miri_detected && usage.clippy_detected {
                // early exit...
                return Ok(usage);
            }
        }
    }

    Ok(usage)
}
