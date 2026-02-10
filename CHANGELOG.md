# cargo-aprz

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
