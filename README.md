# cargo-aprz

[![crate.io](https://img.shields.io/crates/v/cargo-aprz.svg)](https://crates.io/crates/cargo-aprz)
[![CI](https://github.com/geeknoid/cargo-aprz/workflows/main/badge.svg)](https://github.com/geeknoid/cargo-aprz/actions)
[![Coverage](https://codecov.io/gh/geeknoid/cargo-aprz/graph/badge.svg?token=FCUG0EL5TI)](https://codecov.io/gh/geeknoid/cargo-aprz)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

A cargo tool to appraise the quality of Rust dependencies.

- [Background](#background)
- [Installation](#installation)
- [Quick Start](#quick-start)
- [Data Sources](#data-sources)
- [Crates and Dependencies](#crates-and-dependencies)
  - [Dependency Types](#dependency-types)
  - [Package & Feature Selection](#package--feature-selection)
  - [Tokens](#tokens)
- [Reports](#reports)
- [Configuration and Expressions](#configuration-and-expressions)
  - [Expression Checks in CI](#expression-checks-in-ci)
- [Troubleshooting](#troubleshooting)
- [Collected Metrics](#collected-metrics)
  - [Metadata Metrics](#metadata-metrics)
  - [Usage Metrics](#usage-metrics)
  - [Stability Metrics](#stability-metrics)
  - [Community Metrics](#community-metrics)
  - [Activity Metrics](#activity-metrics)
  - [Documentation Metrics](#documentation-metrics)
  - [Advisory Metrics](#advisory-metrics)
  - [Code Metrics](#code-metrics)
  - [Trustworthiness Metrics](#trustworthiness-metrics)

## Background

Building modern applications usually involves integrating a large number of third-party dependencies.
While these dependencies can provide valuable functionality and accelerate development, they also
introduce risks related to quality, security vulnerabilities, future compatibility, and more.

Before taking a dependency in your project, it's useful to vet whether that dependency meets some baseline
quality standards. For example, maybe you believe in having excellent unit test coverage for your projects,
but if you pull in some dependency which has no tests, it can undermine the overall quality of your application.

`cargo-aprz` lets you appraise the quality of dependencies. For any given crate, it collects a large number
of metrics, such as the number of open issues, the frequency of releases, the existence of security advisories,
the number of examples, the code coverage percentage, and many more. You can view nice reports showing you
all of these metrics in an easy to consume form.

You can also use `cargo-aprz` to automatically evaluate whether a crate meets your quality standards. You
do this by writing a set of expressions that operate on the collected metrics. For example, you can have an
expression that says "if code coverage is less than 20%, treat this crate as not being acceptable as a dependency".

You can run `cargo-aprz` by specifying a set of crates to evaluate, or you can run it on the full transitive set of
dependencies of an existing Rust project.

## Installation

```bash
cargo install --locked cargo-aprz
```

## Quick Start

1. Generate a default configuration file:

   ```bash
   cargo aprz init
   ```

   This creates `aprz.toml` which lets you control various options. This is where you define expressions that let you
   evaluate the relative quality of a crate by inspecting its metrics.

2. Get the metrics associated with the latest version of a crate:

   ```bash
   cargo aprz crates tokio
   ```

   The first time you run this command, it will take a while as it needs to download
   a large database from crates.io along with the `RustSec` advisory database. This
   data is cached such that subsequent runs will be much faster.

3. Get the metrics associated with the dependencies of a Rust project:

   ```bash
   cargo aprz deps
   ```

4. Get the metrics for specific versions of crates:

   ```bash
   cargo aprz crates tokio@1.40.0 serde@1.0.0
   ```

5. Get the metrics for a crate and produce an HTML report instead of outputting to the console:

   ```bash
   cargo aprz crates tokio@1.40.0 --html report.html
   ```

## Data Sources

`cargo-aprz` collects data from these sources:

- **crates.io**: Provides metadata and download statistics for each crate.

- **GitHub** or **Codeberg**: Provide information about the popularity of a crate, the
  number of issues and pull requests, the frequency of commits, and more. This is also
  where `cargo-aprz` gets source code in order to analyze the code quality of a crate.

- **`RustSec` Advisory Database**: Provides information about known vulnerabilities in Rust crates.

- **docs.rs**: Provides information about the quality of documentation for a crate, such as the presence of examples,
  the number of items with documentation comments, and more.

- **codecov.io**: Provides code coverage information.

## Crates and Dependencies

`cargo-aprz` can be used to appraise the quality of specific crates, or the quality of the dependencies of an existing Rust project.

When you run `cargo-aprz crates`, you specify a set of crates to appraise with or without a version number. For example:

```bash
cargo aprz crates tokio serde@1.0.1
```

When you run `cargo-aprz deps`, it will appraise the quality of the dependencies of the Rust project in the current directory.

```bash
cargo aprz deps --dependency-types standard
````

### Dependency Types

The `--dependency-types` option accepts a comma-separated list of dependency types to include in the appraisal. Possible values are:

- `standard`: Only include the standard dependencies of the project.
- `dev`: Only include the development dependencies of the project.
- `build`: Only include the build dependencies of the project.

### Package & Feature Selection

When using the `deps` command, you can use the usual cargo options to control precisely which package and feature to consider. The available options include:

- `--manifest-path <PATH>`: Path to the `Cargo.toml` file of the project to analyze. By default, it looks for `Cargo.toml` in the current directory.
- `--features`: A comma-separated list of features to activate.
- `--no-default-features`: Do not activate the `default` feature.
- `--all-features`: Activate all available features.
- `--package`: Appraise the dependencies of a specific package in a workspace.
- `--workspace`: Appraise the dependencies of all packages in a workspace.

### Tokens

`cargo-aprz` accesses he GitHub or Codeberg API to collect data about a crate. Although these APIs can be used without any form of authentication, this
results in very low rate limits. If `cargo-aprz` detects it is being throttled by the API, it will enter a retry loop where it will wait until it is safe
to try the operation again.

When using the `deps` command on a large project, it's likely you'll hit these rate limits, which can make the process take hours to complete fully.
In such a case, you can provide a GitHub or Codebarg token on the command-line or through environment variables, which gives you substantially higher
rate limits.

```bash
cargo aprz deps --github-token <GITHUB_TOKEN> --codeberg-token <CODEBERG_TOKEN>
```

You can also set the `GITHUB_TOKEN` and `CODEBERG_TOKEN` environment variables, which `cargo-aprz` will automatically pick up.

## Reports

When you run `cargo-aprz`, it collects the many metrics listed below and then proceeds to generate a report
that shows all the collected metrics. The report can be in a variety of formats including HTML and JSON.
By default, the report is simply printed to the console.

```bash
cargo aprz crates tokio                     # Terminal output (default)
cargo aprz crates tokio --console           # Terminal output (explicit)
cargo aprz crates tokio --html report.html  # HTML report
cargo aprz crates tokio --json report.json  # JSON data
cargo aprz crates tokio --csv report.csv    # CSV file
cargo aprz crates tokio --excel report.xlsx # Excel spreadsheet
```

## Configuration and Expressions

You can configure `cargo-aprz` by creating an `aprz.toml` file in the current directory. This file lets you define the set of expressions that the tool uses in order
to assess whether a crate is acceptable or not acceptable to use as a dependency. The `--config` option lets you specify an arbitrary path to the configuration file
instead of the default.

`cargo-aprz` uses the [CEL expression language](https://github.com/google/cel-spec/blob/master/doc/langdef.md). This is a flexible, general-purpose expression
language that allows you to write potentially complex boolean expressions that operate on the value of collected metrics. Expressions are divided into two buckets:

- high_risk_if_any: If any of these expressions evaluate to `true`, the crate is flagged as high risk.

- eval: Each expression has a point value (default 1). All expressions are evaluated and a score is
  computed as `granted_points / total_points * 100`. The score is compared against configurable
  thresholds (`medium_risk_threshold` and `low_risk_threshold`) to determine whether the crate is
  low, medium, or high risk.

These buckets are evaluated in order. If no expressions are defined, then all crates are considered low risk.

Within these expressions, you can refer to any of the collected metrics. For example, you could write an expression that says
"if the number of open issues is greater than 100, treat this crate as high risk":

```toml
[[high_risk_if_any]]
name = "Open Issues"
description = "Flags crates with too many open issues, which may indicate an unmaintained or low-quality crate."
expression = "activity.open_issues > 100"   
```

Any of the metric listed in [Collected Metrics](#collected-metrics) below can be used in these expressions, which gives you a lot of flexibility in
defining what you consider to be an acceptable or unacceptable crate.

You can also use `duration()` in expressions for time-based comparisons. You can assign higher point values to more
important expressions using the `points` field:

```toml
[[eval]]
name = "Established Crate"
description = "Accepts if the crate version was created more than 6 months ago."
expression = "stability.version_created_at < (stability.version_updated_at - duration('4320h'))"  # 4320 hours = 180 days
points = 5
```

By default, crates scoring below 30 are high risk, between 30 and 70 are medium risk, and 70 or above are low risk.
You can customize these thresholds:

```toml
medium_risk_threshold = 30.0
low_risk_threshold = 70.0
```

### Expression Checks in CI

If you want to use `cargo-aprz` in a CI pipeline to detect if any unsavory dependencies are being added to your project, you
can use the `--check` option to make `cargo-aprz` return a non-zero exit code if any of the crates being appraised are
flagged as high risk based on the configured expressions.

## Troubleshooting

The `crates` and `deps` commands both let you specify a logging level using the `--log-level` option. Turning on logging can be useful
to troubleshooting connectivity problems. When logging is enabled, then normal console output is suspended.

## Collected Metrics

The sections below show the full set of metrics collected.

### Metadata Metrics

| Metric               | Description                                                |
|----------------------|------------------------------------------------------------|
| `crate.name`         | Name of the crate                                          |
| `crate.version`      | Semantic version of the crate                              |
| `crate.description`  | Description of the crate's purpose and use                 |
| `crate.license`      | SPDX license identifier constraining use of the crate      |
| `crate.categories`   | Crate categories                                           |
| `crate.keywords`     | Crate keywords                                             |
| `crate.features`     | Available crate features                                   |
| `crate.repository`   | URL to the crate's source code repository                  |
| `crate.homepage`     | URL to the crate's homepage                                |
| `crate.minimum_rust` | Minimum Rust version (MSRV) required to compile this crate |
| `crate.rust_edition` | Rust edition this crate targets                            |
| `crate.owners`       | List of owner usernames                                    |

### Usage Metrics

| Metric                                 | Description                                                  |
|----------------------------------------|--------------------------------------------------------------|
| `usage.total_downloads`                | Crate downloads across all versions                          |
| `usage.total_downloads_last_90_days`   | Crate downloads across all versions in the last 90 days      |
| `usage.version_downloads`              | Crate downloads of this specific version                     |
| `usage.version_downloads_last_90_days` | Crate downloads of this specific version in the last 90 days |
| `usage.dependent_crates`               | Number of unique crates that depend on this crate            |

### Stability Metrics

| Metric                             | Description                                                |
|------------------------------------|------------------------------------------------------------|
| `stability.crate_created_at`       | When the crate was first published to crates.io            |
| `stability.crate_updated_at`       | When the crate's metadata was last updated on crates.io    |
| `stability.version_created_at`     | When this version was first published to crates.io         |
| `stability.version_updated_at`     | When this version's metadata was last updated on crates.io |
| `stability.yanked`                 | Whether this version has been yanked from crates.io        |
| `stability.versions_last_90_days`  | Number of versions published in the last 90 days           |
| `stability.versions_last_180_days` | Number of versions published in the last 180 days          |
| `stability.versions_last_365_days` | Number of versions published in the last 365 days          |

### Community Metrics

| Metric                        | Description                                            |
|-------------------------------|--------------------------------------------------------|
| `community.repo_stars`        | Number of stars on the repository                      |
| `community.repo_forks`        | Number of forks of the repository                      |
| `community.repo_subscribers`  | Number of users watching/subscribing to the repository |
| `community.repo_contributors` | Number of contributors to the repository               |

### Activity Metrics

| Metric                                       | Description                                                    |
|----------------------------------------------|----------------------------------------------------------------|
| `activity.commits_last_90_days`              | Number of commits to the repository in the last 90 days        |
| `activity.commits_last_180_days`             | Number of commits to the repository in the last 180 days       |
| `activity.commits_last_365_days`             | Number of commits to the repository in the last 365 days       |
| `activity.commit_count`                      | Total number of commits in the repository                      |
| `activity.last_commit_at`                    | Timestamp of the most recent commit in the repository          |
| `activity.open_issues`                       | Number of currently open issues                                |
| `activity.closed_issues`                     | Total number of issues that have been closed (all time)        |
| `activity.avg_open_issue_age_days`           | Average age in days of open issues                             |
| `activity.median_open_issue_age_days`        | Median age in days of open issues (50th percentile)            |
| `activity.p90_open_issue_age_days`           | 90th percentile age in days of open issues                     |
| `activity.open_pull_requests`                | Number of currently open pull requests                         |
| `activity.closed_pull_requests`              | Total number of pull requests that have been closed (all time) |
| `activity.avg_open_pull_request_age_days`    | Average age in days of open pull requests                      |
| `activity.median_open_pull_request_age_days` | Median age in days of open pull requests (50th percentile)     |
| `activity.p90_open_pull_request_age_days`    | 90th percentile age in days of open pull requests              |

### Documentation Metrics

| Metric                                  | Description                                              |
|-----------------------------------------|----------------------------------------------------------|
| `docs.documentation`                    | URL to the crate's documentation                         |
| `docs.public_api_elements`              | Number of public API elements (functions, structs, etc.) |
| `docs.undocumented_public_api_elements` | Number of public API elements without documentation      |
| `docs.public_api_coverage_percentage`   | Percentage of public API elements with documentation     |
| `docs.crate_level_docs_present`         | Whether crate-level documentation exists                 |
| `docs.broken_links`                     | Number of broken links in documentation                  |
| `docs.examples_in_docs`                 | Number of code examples in documentation                 |
| `docs.standalone_examples`              | Number of standalone example programs in the codebase    |

### Advisory Metrics

| Metric                                                 | Description                                                     |
|--------------------------------------------------------|-----------------------------------------------------------------|
| `advisories.total_low_severity_vulnerabilities`        | Number of low severity vulnerabilities across all versions      |
| `advisories.total_medium_severity_vulnerabilities`     | Number of medium severity vulnerabilities across all versions   |
| `advisories.total_high_severity_vulnerabilities`       | Number of high severity vulnerabilities across all versions     |
| `advisories.total_critical_severity_vulnerabilities`   | Number of critical severity vulnerabilities across all versions |
| `advisories.total_notice_warnings`                     | Number of notice warnings across all versions                   |
| `advisories.total_unmaintained_warnings`               | Number of unmaintained warnings across all versions             |
| `advisories.total_unsound_warnings`                    | Number of unsound warnings across all versions                  |
| `advisories.version_low_severity_vulnerabilities`      | Number of low severity vulnerabilities in this version          |
| `advisories.version_medium_severity_vulnerabilities`   | Number of medium severity vulnerabilities in this version       |
| `advisories.version_high_severity_vulnerabilities`     | Number of high severity vulnerabilities in this version         |
| `advisories.version_critical_severity_vulnerabilities` | Number of critical severity vulnerabilities in this version     |
| `advisories.version_notice_warnings`                   | Number of notice warnings for this version                      |
| `advisories.version_unmaintained_warnings`             | Number of unmaintained warnings for this version                |
| `advisories.version_unsound_warnings`                  | Number of unsound warnings for this version                     |

### Code Metrics

| Metric                          | Description                                          |
|---------------------------------|------------------------------------------------------|
| `code.source_files`             | Number of source files                               |
| `code.source_files_with_errors` | Number of source files that had analysis errors      |
| `code.code_lines`               | Number of lines of production code (excluding tests) |
| `code.test_lines`               | Number of lines of test code                         |
| `code.comment_lines`            | Number of comment lines in the codebase              |
| `code.transitive_dependencies`  | Number of transitive dependencies                    |

### Trustworthiness Metrics

| Metric                           | Description                                             |
|----------------------------------|---------------------------------------------------------|
| `trust.unsafe_blocks`            | Number of unsafe blocks in the codebase                 |
| `trust.ci_workflows`             | Whether CI/CD workflows were detected in the repository |
| `trust.miri_usage`               | Whether Miri is used in CI                              |
| `trust.clippy_usage`             | Whether Clippy is used in CI                            |
| `trust.code_coverage_percentage` | Percentage of code covered by tests                     |
