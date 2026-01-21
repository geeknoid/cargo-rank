//! A tool to analyze and rank Rust crates based on various quality metrics.
//!
//! # Overview
//!
//! `cargo-rank` is a Cargo subcommand that helps you assess whether Rust crates are suitable
//! as dependencies for your project. It collects metrics from crates.io and GitHub, evaluates
//! them against user-defined policies, and produces scores that indicate the quality and
//! health of each dependency.
//!
//! # Installation
//!
//! ```bash
//! cargo install cargo-rank
//! ```
//!
//! # Quick Start
//!
//! Analyze all dependencies in your current project:
//!
//! ```bash
//! cargo rank
//! ```
//!
//! This displays a color-coded console report showing the quality of each dependency.
//!
//! # Basic Usage
//!
//! ## Analyzing Dependencies
//!
//! **Analyze current project:**
//! ```bash
//! cargo rank
//! ```
//!
//! **Analyze specific Cargo.toml:**
//! ```bash
//! cargo rank --manifest-path path/to/Cargo.toml
//! ```
//!
//! **Analyze specific crates directly:**
//! ```bash
//! cargo rank --crate tokio --crate serde --crate reqwest
//! ```
//!
//! ## Workspace Support
//!
//! **Analyze all workspace packages:**
//! ```bash
//! cargo rank --workspace
//! ```
//!
//! **Analyze specific packages:**
//! ```bash
//! cargo rank --package web-server --package api-client
//! cargo rank -p web-server -p api-client  # Short form
//! ```
//!
//! ## Filtering Dependencies
//!
//! **By dependency type:**
//! ```bash
//! cargo rank --dependencies standard      # Only runtime dependencies
//! cargo rank --dependencies dev          # Only dev dependencies
//! cargo rank --dependencies build        # Only build dependencies
//! cargo rank --dependencies standard,dev # Multiple types
//! ```
//!
//! **By feature flags:**
//! ```bash
//! cargo rank --features tokio-compat
//! cargo rank -F feature1,feature2
//! cargo rank --all-features
//! cargo rank --no-default-features --features minimal
//! ```
//!
//! # Output Formats
//!
//! ## Console Output (Default)
//!
//! By default, cargo-rank displays a formatted console report with:
//! - Summary statistics (total crates, average score, quality distribution)
//! - Color-coded table of all dependencies sorted by score
//! - Key issues for each problematic dependency
//! - Actionable recommendations
//!
//! ```bash
//! cargo rank
//! # Shows:
//! # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//! #   Dependency Quality Report
//! # ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//! # Summary:
//! #   Total: 42, Average: 78.5
//! #   ● Excellent: 25 (60%)
//! #   ● Acceptable: 15 (36%)
//! #   ● Needs Attention: 2 (4%)
//! ```
//!
//! ## Detailed Explanations
//!
//! Show detailed score breakdown for each crate:
//!
//! ```bash
//! cargo rank --explain
//! ```
//!
//! This shows:
//! - Overall score and color rating
//! - All policies organized by category (Community, Documentation, Usage, etc.)
//! - Pass/fail status for each policy
//! - Points awarded for each policy
//!
//! ## Report Files
//!
//! **Excel report:**
//! ```bash
//! cargo rank --excel-report deps.xlsx
//! ```
//!
//! **HTML report:**
//! ```bash
//! cargo rank --html-report deps.html
//! ```
//!
//! **Both formats:**
//! ```bash
//! cargo rank --excel-report deps.xlsx --html-report deps.html
//! ```
//!
//! Note: Console output is suppressed when generating report files.
//! Use `--explain` to see detailed output alongside report generation.
//!
//! # CI/CD Integration
//!
//! ## Setting Quality Gates
//!
//! Fail CI builds if dependencies are in the 'bad' quality band:
//!
//! ```bash
//! cargo rank --check
//! ```
//!
//! Exit codes:
//! - `0`: All dependencies pass quality checks
//! - `1`: One or more dependencies are in the 'bad' quality band or mandatory policy failed
//!
//! **Example CI workflow:**
//! ```yaml
//! - name: Check Dependency Quality
//!   run: cargo rank --check --dependencies standard
//!   env:
//!     GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
//! ```
//!
//! ### Mandatory Policies
//!
//! Some policies are considered **mandatory** (points = 0.0) and cause automatic CI failure
//! regardless of the numeric score. A common example is license compliance:
//!
//! - **License policy**: If a dependency's license doesn't match any allowed license,
//!   the crate receives a score of 0 and the quality check will fail.
//!
//! This ensures that critical requirements are always enforced in CI/CD pipelines.
//!
//! **Example:**
//! ```bash
//! # This will fail if any crate has an unacceptable license
//! cargo rank --check
//! ```
//!
//! Configure allowed licenses in your config file:
//! ```toml
//! allowed_licenses = ["MIT", "Apache-2.0", "BSD-3-Clause"]
//! ```
//!
//! ## Validation Only
//!
//! Validate configuration without analyzing crates:
//!
//! ```bash
//! cargo rank --validate-config
//! cargo rank --config custom.toml --validate-config
//! ```
//!
//! This validates the configuration and reports issues such as:
//! - Duplicate policies
//! - Dominated policies (one policy makes another redundant)
//! - Invalid policy values
//!
//! # Configuration
//!
//! ## Using Configuration Files
//!
//! **Specify a config file:**
//! ```bash
//! cargo rank --config rank.toml
//! ```
//!
//! **Default search locations:**
//! - `rank.toml`
//! - `rank.yml`
//! - `rank.yaml`
//! - `rank.json`
//!
//! **Generate default config:**
//! ```bash
//! cargo rank --default-config rank.toml
//! ```
//!
//! ## Configuration Structure
//!
//! Configuration files customize how crates are scored. All configuration fields are optional;
//! unspecified fields use sensible defaults.
//!
//! ### Basic Configuration
//!
//! ```toml
//! # Licenses considered acceptable
//! allowed_licenses = ["MIT", "Apache-2.0", "BSD-3-Clause"]
//!
//! # Score thresholds for color coding
//! overall_scoring_bands = [50.0, 80.0]  # [Orange threshold, Green threshold]
//!                                        # Red: <50, Orange: 50-79, Green: ≥80
//!
//! # Scale factors for specific metrics (optional, rarely needed)
//! # Multiplies the points awarded by policies for specific metrics
//! metric_scaling = {}  # Empty by default
//! ```
//!
//! ### Crate Age Policies
//!
//! Reward crates that have been around long enough to be tested in production.
//!
//! ```toml
//! [[crate_age]]
//! dependency_types = ["Standard"]  # Apply to runtime dependencies
//! min_age = 365                    # At least 1 year old
//! points = 10                      # Award 10 points
//!
//! [[crate_age]]
//! dependency_types = ["Dev", "Build"]
//! min_age = 180                    # 6 months for dev/build deps
//! points = 5
//! ```
//!
//! **Default `dependency_types`:** `["Standard"]` if omitted
//!
//! ### Version Stability Policies
//!
//! Prefer crates with stable (1.0+) versions.
//!
//! ```toml
//! [[crate_stable_versions]]
//! dependency_types = ["Standard"]
//! min_count = 1      # Major version ≥ 1
//! points = 5
//!
//! [[crate_stable_versions]]
//! dependency_types = ["Dev", "Build"]
//! min_count = 1
//! points = 2
//! ```
//!
//! ### Download Popularity Policies
//!
//! Reward widely-adopted crates.
//!
//! ```toml
//! [[crate_downloads_last_month]]
//! min_count = 50000    # At least 50k downloads/month
//! points = 10
//!
//! [[crate_downloads_last_month]]
//! min_count = 10000    # Lower threshold, fewer points
//! points = 5
//!
//! [[crate_downloads_overall]]
//! min_count = 1000000  # Total lifetime downloads
//! points = 8
//! ```
//!
//! ### Ownership Policies
//!
//! Prefer crates maintained by teams or multiple maintainers.
//!
//! ```toml
//! [[crate_owners_teams]]
//! dependency_types = ["Standard"]
//! min_count = 1      # At least one team owns it
//! points = 15
//!
//! [[crate_owners_users]]
//! min_count = 3      # Or multiple individual maintainers
//! points = 10
//! ```
//!
//! ### Release Activity Policies
//!
//! Reward crates with recent releases.
//!
//! ```toml
//! [[crate_releases]]
//! min_count = 1      # At least 1 release
//! min_age = 180      # Within last 6 months
//! points = 8
//!
//! [[crate_releases]]
//! min_count = 3      # Multiple recent releases
//! min_age = 365      # Within last year
//! points = 12
//! ```
//!
//! ### Repository Activity Policies
//!
//! Reward active development and maintenance.
//!
//! ```toml
//! [[repo_contributors]]
//! min_count = 10     # At least 10 contributors
//! points = 8
//!
//! [[repo_commits]]
//! min_count = 5      # At least 5 commits
//! min_age = 90       # Within last 3 months
//! points = 10
//!
//! [[repo_commits]]
//! min_count = 1      # Any commit in last 3 months
//! min_age = 90
//! points = 3
//! ```
//!
//! ### Issue Responsiveness Policies
//!
//! Reward projects that close issues quickly.
//!
//! ```toml
//! [[repo_issue_responsiveness]]
//! percentile = 50    # Median (p50) close time
//! max_days = 30      # Under 30 days
//! points = 5
//!
//! [[repo_issue_responsiveness]]
//! percentile = 75    # 75th percentile
//! max_days = 90      # Under 90 days
//! points = 3
//! ```
//!
//! Supported percentiles: 50, 75, 90
//!
//! ### Pull Request Responsiveness Policies
//!
//! Reward projects that merge PRs quickly.
//!
//! ```toml
//! [[repo_pull_request_responsiveness]]
//! percentile = 50
//! max_days = 7       # Median PR merged within a week
//! points = 8
//!
//! [[repo_pull_request_responsiveness]]
//! percentile = 90
//! max_days = 30      # 90% of PRs merged within a month
//! points = 5
//! ```
//!
//! ## Dependency Type Filtering
//!
//! All policies support dependency type filtering:
//!
//! - `Standard`: Runtime dependencies (default)
//! - `Dev`: Development dependencies
//! - `Build`: Build dependencies
//!
//! **Examples:**
//! ```toml
//! [[crate_age]]
//! dependency_types = ["Standard", "Build"]  # Both runtime and build
//! min_age = 180
//! points = 5
//!
//! [[crate_age]]
//! # No dependency_types = defaults to ["Standard"]
//! min_age = 365
//! points = 10
//! ```
//!
//! ## Configuration Validation
//!
//! cargo-rank automatically detects common configuration problems:
//!
//! **Duplicate policies:**
//! ```toml
//! [[crate_age]]
//! min_age = 365
//! points = 10
//!
//! [[crate_age]]
//! min_age = 365  # ⚠️ Warning: duplicate threshold
//! points = 10
//! ```
//!
//! **Dominated policies (unreachable):**
//! ```toml
//! [[crate_age]]
//! min_age = 180
//! points = 15
//!
//! [[crate_age]]
//! min_age = 365  # ⚠️ Warning: dominated by min_age=180
//! points = 10    # Lower threshold gives more points!
//! ```
//!
//! Validation warnings are printed to stderr but don't prevent execution.
//! Use `--validate-config` to check configuration without analyzing crates.
//!
//! # Configuration Examples
//!
//! ## Strict Production Requirements
//!
//! High standards for production dependencies:
//!
//! ```toml
//! allowed_licenses = ["MIT", "Apache-2.0"]
//! overall_scoring_bands = [75.0, 90.0]
//! metric_scaling = {}
//!
//! [[crate_age]]
//! dependency_types = ["Standard"]
//! min_age = 730  # 2 years minimum
//! points = 20
//!
//! [[crate_owners_teams]]
//! dependency_types = ["Standard"]
//! min_count = 1  # Must be team-maintained
//! points = 25
//!
//! [[crate_downloads_last_month]]
//! min_count = 100000  # High adoption
//! points = 20
//!
//! [[repo_commits]]
//! min_count = 10
//! min_age = 90  # Active maintenance
//! points = 15
//! ```
//!
//! ## Lenient Dev Dependencies
//!
//! Relaxed standards for development tools:
//!
//! ```toml
//! overall_scoring_bands = [40.0, 70.0]
//!
//! [[crate_age]]
//! dependency_types = ["Dev", "Build"]
//! min_age = 90   # 3 months minimum
//! points = 5
//!
//! [[crate_owners_users]]
//! dependency_types = ["Dev", "Build"]
//! min_count = 1  # Single maintainer OK
//! points = 5
//! ```
//!
//! ## Focus on Maintenance Activity
//!
//! Prioritize actively maintained crates:
//!
//! ```toml
//! [[repo_commits]]
//! min_count = 10
//! min_age = 90  # High weight for recent activity
//! points = 20
//!
//! [[repo_issue_responsiveness]]
//! percentile = 50
//! max_days = 14  # Fast issue resolution
//! points = 15
//!
//! [[repo_pull_request_responsiveness]]
//! percentile = 50
//! max_days = 3  # Very fast PR merging
//! points = 15
//!
//! [[crate_releases]]
//! min_count = 1
//! min_age = 90  # Recent release required
//! points = 10
//! ```
//!
//! # GitHub Integration
//!
//! GitHub repository metrics significantly improve scoring accuracy. Without GitHub data,
//! you'll miss metrics like commit activity, issue/PR responsiveness, and contributor counts.
//!
//! ## Setting Up GitHub Access
//!
//! 1. Create a personal access token at <https://github.com/settings/tokens>
//! 2. No special permissions needed (public repo access is sufficient)
//! 3. Provide the token via environment variable or command-line flag
//!
//! **Environment variable (recommended):**
//! ```bash
//! export GITHUB_TOKEN=ghp_xxxxxxxxxxxxxxxxxxxx
//! cargo rank
//! ```
//!
//! **Command-line flag:**
//! ```bash
//! cargo rank --hosting-token ghp_xxxxxxxxxxxxxxxxxxxx
//! ```
//!
//! # Scoring System
//!
//! ## How Scores Are Calculated
//!
//! Each crate starts at 0 points. Policies in your configuration file (or defaults) define
//! conditions that award points. The final score is the sum of all awarded points, capped
//! at 100.
//!
//! **Mandatory policies:**
//! - **License compliance**: License must match one of the allowed licenses
//!   - If this policy fails, the crate receives a score of 0
//!   - Quality checks will fail immediately
//!   - This ensures license compliance cannot be bypassed
//!
//! **Quality categories:**
//! - **Maturity**: Crate age, version stability
//! - **Popularity**: Download counts, dependent crates
//! - **Maintenance**: Recent releases, commit activity
//! - **Community**: Contributors, ownership structure
//! - **Responsiveness**: Issue/PR handling speed
//!
//! ## Color Ratings
//!
//! Scores are color-coded based on thresholds (configurable):
//!
//! - 🟢 **Green (≥80)**: Excellent quality, safe for production
//! - 🟠 **Orange (50-79)**: Acceptable with some concerns, review recommended
//! - 🔴 **Red (<50)**: Significant quality issues, use with caution
//! - ⚫ **Gray**: Missing data (crate not found or API errors)
//!
//! # Complete Examples
//!
//! ## Example 1: Pre-deployment Check
//!
//! Check all runtime dependencies meet production standards:
//!
//! ```bash
//! cargo rank \
//!   --dependencies standard \
//!   --check \
//!   --config production-rank.toml \
//!   --excel-report pre-deploy-audit.xlsx
//! ```
//!
//! ## Example 2: Weekly Quality Report
//!
//! Generate comprehensive reports for team review:
//!
//! ```bash
//! export GITHUB_TOKEN=$GITHUB_PAT
//! cargo rank \
//!   --workspace \
//!   --all-features \
//!   --html-report weekly-report.html \
//!   --excel-report weekly-report.xlsx
//! ```
//!
//! ## Example 3: Investigate Low-Quality Dependencies
//!
//! Find and understand problematic dependencies:
//!
//! ```bash
//! # First, identify problems
//! cargo rank --dependencies standard
//!
//! # Then, get detailed explanations
//! cargo rank --explain > dependency-audit.txt
//! ```
//!
//! ## Example 4: CI Pipeline Integration
//!
//! GitHub Actions workflow:
//!
//! ```yaml
//! name: Dependency Quality Check
//!
//! on: [push, pull_request]
//!
//! jobs:
//!   check-deps:
//!     runs-on: ubuntu-latest
//!     steps:
//!       - uses: actions/checkout@v3
//!       - uses: actions-rust-lang/setup-rust-toolchain@v1
//!
//!       - name: Install cargo-rank
//!         run: cargo install cargo-rank
//!
//!       - name: Validate configuration
//!         run: cargo rank --config .cargo/rank.toml --validate-config
//!
//!       - name: Check dependency quality
//!         env:
//!           GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
//!         run: |
//!           cargo rank \
//!             --dependencies standard \
//!             --check \
//!             --excel-report deps-report.xlsx
//!
//!       - name: Upload report
//!         uses: actions/upload-artifact@v3
//!         if: always()
//!         with:
//!           name: dependency-report
//!           path: deps-report.xlsx
//! ```
//!
//! ## Example 5: Compare Competing Crates
//!
//! Evaluate alternatives before choosing a dependency:
//!
//! ```bash
//! cargo rank \
//!   --crate tokio \
//!   --crate async-std \
//!   --crate smol \
//!   --explain
//! ```
//!
//! ## Example 6: Audit Specific Package in Workspace
//!
//! Focus on one package's dependencies:
//!
//! ```bash
//! cargo rank \
//!   --package my-api-server \
//!   --dependencies standard \
//!   --config strict-api-deps.toml \
//!   --check
//! ```
//!
//! # Troubleshooting
//!
//! ## No Console Output
//!
//! Console output is suppressed when generating reports. Either:
//! - Run without `--html-report` or `--excel-report` to see console output
//! - Use `--explain` to see detailed output alongside reports
//!
//! ## GitHub API Rate Limiting
//!
//! Public (unauthenticated) GitHub API has strict rate limits. Solutions:
//! - Provide a GitHub token via `GITHUB_TOKEN` environment variable
//! - Tokens increase rate limit from 60 to 5000 requests/hour
//!
//! ## Missing Crate Data
//!
//! If a crate shows as Gray or missing:
//! - Crate might not exist on crates.io
//! - Network connectivity issues
//! - crates.io API temporarily unavailable
//! - Check crate name spelling
//!
//! ## Configuration Warnings
//!
//! Validation warnings (⚠️) indicate non-optimal config but don't prevent execution:
//! - Review and fix dominated policies
//! - Remove duplicate policies
//! - Use `--validate-config` to check before running full analysis

use cargo_rank::Result;
use clap::builder::Styles;
use clap::builder::styling::{AnsiColor, Effects};
use clap::{Parser, Subcommand};

mod commands;

use crate::commands::{
    ConvertArgs, CratesArgs, DepsArgs, InitArgs, ValidateArgs, convert_config, init_config, process_crates, process_dependencies,
    validate_config,
};

const CLAP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default());

#[derive(Parser, Debug)]
#[command(name = "cargo-rank", bin_name = "cargo", version, about)]
#[command(styles = CLAP_STYLES)]
struct Cli {
    #[command(subcommand)]
    command: CargoSubcommand,
}

#[derive(Subcommand, Debug)]
enum CargoSubcommand {
    Rank(Args),
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: RankSubcommand,
}

#[derive(Subcommand, Debug)]
enum RankSubcommand {
    /// Analyze specific crates and generate quality reports
    Crates(Box<CratesArgs>),
    /// Analyze workspace dependencies and generate quality reports
    Deps(Box<DepsArgs>),
    /// Generate a default configuration file
    Init(InitArgs),
    /// Validate a configuration file
    Validate(ValidateArgs),
    /// Convert a configuration file between formats
    Convert(ConvertArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let CargoSubcommand::Rank(args) = Cli::parse().command;

    match &args.command {
        RankSubcommand::Crates(crates_args) => process_crates(crates_args).await,
        RankSubcommand::Deps(deps_args) => process_dependencies(deps_args).await,
        RankSubcommand::Init(init_args) => init_config(init_args),
        RankSubcommand::Validate(validate_args) => validate_config(validate_args),
        RankSubcommand::Convert(convert_args) => convert_config(convert_args),
    }
}
