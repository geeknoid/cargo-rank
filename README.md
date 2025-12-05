# cargo-rank

[![crate.io](https://img.shields.io/crates/v/cargo-rank.svg)](https://crates.io/crates/cargo-rank)
[![docs.rs](https://docs.rs/cargo-rank/badge.svg)](https://docs.rs/cargo-rank)
[![CI](https://github.com/geeknoid/cargo-rank/workflows/main/badge.svg)](https://github.com/geeknoid/cargo-rank/actions)
[![Coverage](https://codecov.io/gh/geeknoid/cargo-rank/graph/badge.svg?token=FCUG0EL5TI)](https://codecov.io/gh/geeknoid/cargo-rank)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](./LICENSE)

* [Summary](#summary)
* [Overview](#overview)
* [Installation](#installation)
* [Basic Usage](#basic-usage)
  * [Analyze Dependencies from Cargo.toml](#analyze-dependencies-from-cargotoml)
  * [Analyze Specific Crates](#analyze-specific-crates)
  * [Working with Workspaces](#working-with-workspaces)
  * [Filtering by Dependency Type](#filtering-by-dependency-type)
  * [Feature Selection](#feature-selection)
  * [Configuration File](#configuration-file)
  * [Output Formats](#output-formats)
    * [Excel Report](#excel-report)
    * [HTML Report](#html-report)
    * [Multiple Reports](#multiple-reports)
* [GitHub Integration](#github-integration)
* [Scoring Metrics](#scoring-metrics)
* [Examples](#examples)
  * [Comprehensive Analysis](#comprehensive-analysis)
  * [Quick Check](#quick-check)
  * [CI/CD Integration](#cicd-integration)
  * [Advanced Usage](#advanced-usage)

## Summary

<!-- cargo-rdme start -->

cargo-rank library

This library provides functionality for analyzing and ranking Rust crates to help assess
whether they are suitable as dependencies for your project.

## How It Works

cargo-rank operates in three stages:

1. **Metrics Collection**: Gathers data about each crate from crates.io and GitHub,
   including download counts, ownership, documentation, commit activity, issue
   responsiveness, and more.

2. **Policy Evaluation**: Applies user-defined policies to each metric. Policies define
   thresholds (e.g., "at least 2 owners") and award points when the metric meets the
   criteria.

3. **Score Aggregation**: Combines points from all policies into category scores
   (Community, Documentation, Usage, Ownership, Stability, Trustworthiness, Activity,
   Cost) and an overall score (0-100) that indicates whether the crate is acceptable
   as a dependency.

<!-- cargo-rdme end -->
