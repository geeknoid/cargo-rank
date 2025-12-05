# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

cargo-rank is a Cargo subcommand that analyzes and ranks Rust crates based on quality metrics to help assess whether they are suitable as dependencies. It collects data from crates.io and GitHub, applies user-defined policies, and produces scores (0-100) indicating crate quality.

## Build and Development Commands

### Building
```bash
cargo build                 # Debug build
cargo build --release       # Release build with optimizations
```

### Testing
```bash
cargo test                  # Run all tests
cargo test <test_name>      # Run specific test
cargo test --test cli_tests # Run specific test file
```

### Linting and Formatting
```bash
cargo clippy                # Run clippy lints (extensive rules in Cargo.toml)
cargo fmt                   # Format codebase
cargo fmt -- --check        # Check formatting without modifying
```

### Running the Tool
```bash
cargo run                           # Analyze current project dependencies
cargo run -- --crate tokio          # Analyze specific crate
cargo run -- --help                 # See all options
```

### Configuration Commands
```bash
cargo run -- init                   # Generate default config file
cargo run -- validate               # Validate configuration
cargo run -- convert <file>         # Convert config between formats
```

## High-Level Architecture

### Three-Stage Pipeline

1. **Metrics Collection** (`src/facts/`)
   - `collector.rs` orchestrates parallel API fetching
   - `crates_io/` - Fetches crate metadata, versions, owners, downloads, dependencies
   - `github/` - Fetches commit activity, issue/PR stats, responsiveness data
   - Produces `CrateFacts` structure combining all data

2. **Policy Evaluation** (`src/ranking/`)
   - `metric_calculator.rs` evaluates 39 metrics against configured policies
   - Each metric applies typed policies (AgePolicy, MinCountPolicy, LicensePolicy, etc.)
   - Returns `PolicyOutcome` (Match/NoMatch) for each policy
   - `ranker.rs` aggregates outcomes into category scores and overall score (0-100)

3. **Report Generation** (`src/reports/`)
   - Console: Color-coded terminal table with summary statistics
   - Excel: Spreadsheet with detailed metrics and scores
   - HTML: Interactive report with charts

### Module Responsibilities

- **`commands/`** - CLI command handlers (deps, crates, init, validate, convert)
- **`config/`** - Configuration system with 10 policy types and validation logic
- **`facts/`** - Data collection layer (API clients for crates.io and GitHub)
- **`metrics/`** - 39 metric definitions organized into 8 categories
- **`ranking/`** - Scoring engine that evaluates policies and calculates scores
- **`reports/`** - Output formatters (console, Excel, HTML)
- **`misc/`** - Utilities (color modes, dependency types)

### Key Data Structures

**CrateFacts** - Central data structure combining:
- `version: Version` - Semantic version being analyzed
- `crate_data: CrateData` - crates.io metadata (versions, owners, downloads, dependencies)
- `repo_data: Option<RepoData>` - GitHub data (commits, issues, PRs, activity stats)
- `dependency_type: DependencyType` - Standard/Dev/Build classification

**RankingOutcome** - Scoring results:
- `overall_score: f64` - Overall quality score (0-100)
- `category_scores: HashMap<MetricCategory, f64>` - Scores for 8 categories
- `details: Vec<(Metric, Vec<PolicyOutcome>)>` - Per-metric policy match details

### Policy System Design

The policy system uses a trait-based approach with 10 concrete implementations:

- `AgePolicy` - Time-based thresholds (min_days)
- `VersionPolicy` - Major version requirements
- `MinCountPolicy` / `MaxCountPolicy` - Count thresholds
- `AgedCountPolicy` - Count within time window
- `PercentagePolicy` - Percentage range checks
- `BooleanPolicy` - Binary checks (safe/unsafe code)
- `LicensePolicy` - String matching for allowed licenses
- `ResponsivenessPolicy` - Age percentiles (p50, p75, p90, p95)

Each policy specifies:
- `dependency_types()` - Which dependency types it applies to
- `points()` - How many points to award on match
- `validate()` - Configuration validation logic

### Configuration System

Configuration is loaded from `default_config.yml` (embedded) or user-provided YAML/TOML files. The build script (`build.rs`) validates the default config at compile time.

**Key config elements:**
- Policy definitions for each metric (40+ policy arrays)
- `metric_scaling` - Per-metric point multipliers
- `overall_scoring_bands` - Thresholds for color coding [orange, green]
- `category_scoring_bands` - Per-category thresholds

The validator detects:
- Dominated policies (one policy makes another unreachable)
- Duplicate policies
- Invalid thresholds

## Important Implementation Notes

### API Integration

**GitHub API** (`src/facts/github/github_client.rs`):
- Uses octocrab + raw reqwest for different endpoints
- Implements rate limiting with mutex-protected sleep
- Gracefully degrades if GitHub data unavailable (doesn't fail analysis)
- Fetches data in 3-month windows for efficiency
- Calculates issue/PR age percentiles (p50, p75, p90, p95)

**crates.io API** (`src/facts/crates_io/`):
- Uses reqwest with mutex serialization to prevent rate limiting
- Parallel requests for different data types
- Required data - analysis fails if unavailable

### Metric Evaluation Flow

1. Extract metric value from `CrateFacts`
2. Filter policies by `dependency_type`
3. Test each policy predicate in order
4. Return on first match (policies should be ordered by strictness)
5. Apply `metric_scaling` factor to points
6. Record `PolicyOutcome` with detailed information

### Scoring Algorithm

- Sum all matched points per metric
- Calculate average across metrics (not points)
- Cap at 100.0
- Compute category averages separately
- Quality gate fails if any crate in lowest scoring band

### Code Quality Standards

This project enforces extensive linting (see `Cargo.toml` lints section):
- All Clippy lint groups enabled (cargo, complexity, correctness, nursery, pedantic, perf, style)
- Many restriction group lints enabled (unwrap_used, panic, etc.)
- No `unwrap()` calls allowed - use proper error handling
- Safety comments required for unsafe blocks
- Comprehensive Rust and Clippy warnings enabled

When the copilot-instructions.md references "review", apply the Rust API Guidelines and Pragmatic Rust Guidelines documented in that file.

## Testing Structure

Tests are in `tests/`:
- `cli_tests.rs` - Integration tests for CLI commands
- `config_tests.rs` - Configuration loading and validation
- `scoring_tests.rs` - Policy evaluation and scoring logic
- `console_output_tests.rs` - Output formatting tests
- `ci_gate_tests.rs` - Quality gate behavior tests
- `common/` - Test utilities including mock repository support

Use `assert_cmd` and `predicates` crates for CLI testing.

## Build System

`build.rs` validates `default_config.yml` at compile time:
- Loads configuration using same logic as runtime
- Reports validation warnings as build errors
- Ensures default config is always valid
- Triggers rebuild when config files change

## Rust Edition

Uses Rust 2024 edition (minimum version 1.91). This is a recent edition, so be aware of any edition-specific features when making changes.
