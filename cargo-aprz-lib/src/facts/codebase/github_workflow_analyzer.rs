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
            log::warn!(target: LOG_TARGET, "Workflow file count limit ({MAX_WORKFLOW_FILES}) exceeded in directory '{}', stopping scan", workflows_dir.display());
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_no_workflows_directory() {
        let temp_dir = env::temp_dir().join("test_no_workflows");
        let _ = fs::create_dir_all(&temp_dir);

        let result = sniff_github_workflows(&temp_dir).unwrap();

        assert!(!result.workflows_detected);
        assert!(!result.miri_detected);
        assert!(!result.clippy_detected);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_empty_workflows_directory() {
        let temp_dir = env::temp_dir().join("test_empty_workflows");
        let workflows_dir = temp_dir.join(".github").join("workflows");
        let _ = fs::create_dir_all(&workflows_dir);

        let result = sniff_github_workflows(&temp_dir).unwrap();

        assert!(result.workflows_detected);
        assert!(!result.miri_detected);
        assert!(!result.clippy_detected);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_workflows_with_clippy() {
        let temp_dir = env::temp_dir().join("test_clippy_workflow");
        let workflows_dir = temp_dir.join(".github").join("workflows");
        let _ = fs::create_dir_all(&workflows_dir);

        let workflow_file = workflows_dir.join("ci.yml");
        fs::write(
            &workflow_file,
            "
name: CI
on: [push]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Run clippy
        run: cargo clippy -- -D warnings
",
        )
        .unwrap();

        let result = sniff_github_workflows(&temp_dir).unwrap();

        assert!(result.workflows_detected);
        assert!(result.clippy_detected);
        assert!(!result.miri_detected);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_workflows_with_miri() {
        let temp_dir = env::temp_dir().join("test_miri_workflow");
        let workflows_dir = temp_dir.join(".github").join("workflows");
        let _ = fs::create_dir_all(&workflows_dir);

        let workflow_file = workflows_dir.join("miri.yaml");
        fs::write(
            &workflow_file,
            "
name: Miri
on: [push]
jobs:
  miri:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - name: Run Miri
        run: cargo +nightly miri test
",
        )
        .unwrap();

        let result = sniff_github_workflows(&temp_dir).unwrap();

        assert!(result.workflows_detected);
        assert!(!result.clippy_detected);
        assert!(result.miri_detected);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_workflows_with_both() {
        let temp_dir = env::temp_dir().join("test_both_workflow");
        let workflows_dir = temp_dir.join(".github").join("workflows");
        let _ = fs::create_dir_all(&workflows_dir);

        let workflow_file = workflows_dir.join("ci.yml");
        fs::write(
            &workflow_file,
            "
name: CI
on: [push]
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - name: Run Clippy
        run: cargo clippy
  miri:
    runs-on: ubuntu-latest
    steps:
      - name: Run Miri
        run: cargo +nightly miri test
",
        )
        .unwrap();

        let result = sniff_github_workflows(&temp_dir).unwrap();

        assert!(result.workflows_detected);
        assert!(result.clippy_detected);
        assert!(result.miri_detected);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_case_insensitive_detection() {
        let temp_dir = env::temp_dir().join("test_case_insensitive");
        let workflows_dir = temp_dir.join(".github").join("workflows");
        let _ = fs::create_dir_all(&workflows_dir);

        let workflow_file = workflows_dir.join("ci.yml");
        fs::write(
            &workflow_file,
            "
name: CI
steps:
  - name: Run CLIPPY in uppercase
    run: cargo CLIPPY
  - name: Run MiRi in mixed case
    run: cargo MiRi test
",
        )
        .unwrap();

        let result = sniff_github_workflows(&temp_dir).unwrap();

        assert!(result.clippy_detected);
        assert!(result.miri_detected);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_multiple_workflow_files() {
        let temp_dir = env::temp_dir().join("test_multiple_workflows");
        let workflows_dir = temp_dir.join(".github").join("workflows");
        let _ = fs::create_dir_all(&workflows_dir);

        // First file with clippy
        fs::write(workflows_dir.join("clippy.yml"), "run: cargo clippy").unwrap();

        // Second file with miri
        fs::write(workflows_dir.join("miri.yaml"), "run: cargo miri test").unwrap();

        // Third file with neither
        fs::write(workflows_dir.join("test.yml"), "run: cargo test").unwrap();

        let result = sniff_github_workflows(&temp_dir).unwrap();

        assert!(result.workflows_detected);
        assert!(result.clippy_detected);
        assert!(result.miri_detected);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetTempPathW")]
    fn test_non_yaml_files_ignored() {
        let temp_dir = env::temp_dir().join("test_non_yaml_files");
        let workflows_dir = temp_dir.join(".github").join("workflows");
        let _ = fs::create_dir_all(&workflows_dir);

        // Create a non-YAML file with clippy/miri mentions
        fs::write(workflows_dir.join("README.md"), "This mentions clippy and miri").unwrap();

        // Create a YAML file without mentions
        fs::write(workflows_dir.join("ci.yml"), "run: cargo test").unwrap();

        let result = sniff_github_workflows(&temp_dir).unwrap();

        assert!(result.workflows_detected);
        // README.md should be ignored
        assert!(!result.clippy_detected);
        assert!(!result.miri_detected);

        // Cleanup
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_github_workflow_info_default() {
        let info = GitHubWorkflowInfo::default();
        assert!(!info.workflows_detected);
        assert!(!info.clippy_detected);
        assert!(!info.miri_detected);
    }

    #[test]
    fn test_github_workflow_info_clone() {
        let info1 = GitHubWorkflowInfo {
            workflows_detected: true,
            clippy_detected: true,
            miri_detected: false,
        };

        let info2 = info1.clone();

        assert_eq!(info1.workflows_detected, info2.workflows_detected);
        assert_eq!(info1.clippy_detected, info2.clippy_detected);
        assert_eq!(info1.miri_detected, info2.miri_detected);
    }
}
