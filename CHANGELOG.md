# cargo-aprz

## 0.4.0 [TBD]

### Added

- New metrics:
  - `stability.versions_last_180_days`
  - `stability.versions_last_365_days`
  - `activity.commits_last_180_days`
  - `activity.first_commit_at`
  - `activity.open_issues`
  - `activity.open_issue_age_avg`
  - `activity.open_issue_age_p50`
  - `activity.open_issue_age_p75`
  - `activity.open_issue_age_p90`
  - `activity.open_issue_age_p95`
  - `activity.issues_opened_last_90_days`
  - `activity.issues_opened_last_180_days`
  - `activity.issues_opened_last_365_days`
  - `activity.issues_opened_total`
  - `activity.issues_closed_last_90_days`
  - `activity.issues_closed_last_180_days`
  - `activity.issues_closed_last_365_days`
  - `activity.issues_closed_total`
  - `activity.closed_issue_age_avg`
  - `activity.closed_issue_age_p50`
  - `activity.closed_issue_age_p75`
  - `activity.closed_issue_age_p90`
  - `activity.closed_issue_age_p95`
  - `activity.closed_issue_age_last_90_days_avg`
  - `activity.closed_issue_age_last_90_days_p50`
  - `activity.closed_issue_age_last_90_days_p75`
  - `activity.closed_issue_age_last_90_days_p90`
  - `activity.closed_issue_age_last_90_days_p95`
  - `activity.closed_issue_age_last_180_days_avg`
  - `activity.closed_issue_age_last_180_days_p50`
  - `activity.closed_issue_age_last_180_days_p75`
  - `activity.closed_issue_age_last_180_days_p90`
  - `activity.closed_issue_age_last_180_days_p95`
  - `activity.closed_issue_age_last_365_days_avg`
  - `activity.closed_issue_age_last_365_days_p50`
  - `activity.closed_issue_age_last_365_days_p75`
  - `activity.closed_issue_age_last_365_days_p90`
  - `activity.closed_issue_age_last_365_days_p95`
  - `activity.open_prs`
  - `activity.open_pr_age_avg`
  - `activity.open_pr_age_p50`
  - `activity.open_pr_age_p75`
  - `activity.open_pr_age_p90`
  - `activity.open_pr_age_p95`
  - `activity.prs_opened_last_90_days`
  - `activity.prs_opened_last_180_days`
  - `activity.prs_opened_last_365_days`
  - `activity.prs_opened_total`
  - `activity.prs_merged_last_90_days`
  - `activity.prs_merged_last_180_days`
  - `activity.prs_merged_last_365_days`
  - `activity.prs_merged_total`
  - `activity.prs_closed_last_90_days`
  - `activity.prs_closed_last_180_days`
  - `activity.prs_closed_last_365_days`
  - `activity.prs_closed_total`
  - `activity.merged_pr_age_avg`
  - `activity.merged_pr_age_p50`
  - `activity.merged_pr_age_p75`
  - `activity.merged_pr_age_p90`
  - `activity.merged_pr_age_p95`
  - `activity.merged_pr_age_last_90_days_avg`
  - `activity.merged_pr_age_last_90_days_p50`
  - `activity.merged_pr_age_last_90_days_p75`
  - `activity.merged_pr_age_last_90_days_p90`
  - `activity.merged_pr_age_last_90_days_p95`
  - `activity.merged_pr_age_last_180_days_avg`
  - `activity.merged_pr_age_last_180_days_p50`
  - `activity.merged_pr_age_last_180_days_p75`
  - `activity.merged_pr_age_last_180_days_p90`
  - `activity.merged_pr_age_last_180_days_p95`
  - `activity.merged_pr_age_last_365_days_avg`
  - `activity.merged_pr_age_last_365_days_p50`
  - `activity.merged_pr_age_last_365_days_p75`
  - `activity.merged_pr_age_last_365_days_p90`
  - `activity.merged_pr_age_last_365_days_p95`

### Removed

- Removed metrics (replaced by new naming convention above):
  - `activity.closed_issues`
  - `activity.closed_pull_requests`
  - `activity.open_pull_requests`
  - `activity.avg_open_issue_age_days`
  - `activity.median_open_issue_age_days`
  - `activity.p90_open_issue_age_days`
  - `activity.avg_open_pull_request_age_days`
  - `activity.median_open_pull_request_age_days`
  - `activity.p90_open_pull_request_age_days`

## 0.3.0 2026-02-10

### Changed

- Replaced binary accept/deny evaluation model with three-level risk assessment (`Low`, `Medium`, `High`).
- Renamed the `deny_if_any` expressions in config to `high_risk_if_any`.
- Replaced the `accept_if_all' and `accept_if_any` expressions in config with `eval'

## 0.2.0 2026-02-09

### Added

- New metrics: `activity.commits_last_365_days`, `activity.commit_count`, and `activity.last_commit_at`.
- Commit data (`commits_last_90_days`, `commits_last_365_days`, `commit_count`, `last_commit_at`) is now collected from the local git repository instead of the hosting API, reducing API usage.

### Fixed

- Clamp negative timestamps and epoch days to 0 before unsigned conversion, preventing wraparound for pre-epoch dates.
- Fixed `deps` command not handling multiple instances of the same crate with different versions.
- CSV escaping now handles all edge cases correctly.
- Fixed `--dependency-types` to only apply to the first level of dependencies.
- Fixed inconsistencies between CWD and standard Cargo path semantics.
- Fixed encoding issues in generated reports.
- Fixed evaluation to pass when no expressions are defined, matching documented behavior.

### Changed

- Eliminated `--eval` command-line option; expressions in configuration are now always evaluated.

## 0.1.0 2026-02-08

- Initial release.
