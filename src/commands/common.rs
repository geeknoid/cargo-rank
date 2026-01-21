//! Common processing logic shared between crates and deps commands.

use camino::Utf8PathBuf;
use cargo_metadata::MetadataCommand;
use cargo_rank::Result;
use cargo_rank::config::Config;
use cargo_rank::facts::ProgressReporter;
use cargo_rank::facts::{Collector, CrateFacts, CrateRef, ProviderResult};
use cargo_rank::misc::{ColorMode, DependencyType};
use cargo_rank::ranking::Ranker;
use cargo_rank::ranking::RankingOutcome;
use cargo_rank::reports::{generate_console, generate_html, generate_xlsx};
use clap::Args;
use clap::ValueEnum;
use core::time::Duration;
use directories::BaseDirs;
use ohno::{IntoAppError, bail};
use std::fs;

/// Log level for diagnostic output
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    /// No logging output
    None,
    /// Only error messages
    Error,
    /// Warning and error messages
    Warn,
    /// Info, warning, and error messages
    Info,
    /// Debug and above messages
    Debug,
    /// All messages including trace
    Trace,
}

/// Common arguments shared between crates and deps commands
#[derive(Args, Debug)]
pub struct CommonArgs {
    /// GitHub personal access token
    #[arg(long, value_name = "TOKEN", env = "GITHUB_TOKEN")]
    pub github_token: Option<String>,

    /// Path to Cargo.toml file
    #[arg(long, default_value = "Cargo.toml", value_name = "PATH")]
    pub manifest_path: Utf8PathBuf,

    /// Path to configuration file [default: one of rank.[toml|yml|yaml|json] ]
    #[arg(long, short = 'c', value_name = "PATH")]
    pub config: Option<Utf8PathBuf>,

    /// Show only a single line per crate with name, version, and score
    #[arg(long)]
    pub short: bool,

    /// Exit with failure if any crates are in the 'bad' quality band
    #[arg(long)]
    pub check: bool,

    /// Control when to use colored output
    #[arg(long, value_name = "WHEN", default_value = "auto")]
    pub color: ColorMode,

    /// Directory where crate facts are cached [default: ~/.cargo/cargo-rank]
    #[arg(long, value_name = "PATH")]
    pub cache_dir: Option<Utf8PathBuf>,

    /// Set the logging level for diagnostic output
    #[arg(long, value_name = "LEVEL", default_value = "none", global = true)]
    pub log_level: LogLevel,

    /// Output crate information to an Excel spreadsheet file instead of to the terminal
    #[arg(long, value_name = "PATH", help_heading = "Report Output")]
    pub excel: Option<Utf8PathBuf>,

    /// Output crate information to an HTML file instead of to the terminal
    #[arg(long, value_name = "PATH", help_heading = "Report Output")]
    pub html: Option<Utf8PathBuf>,
}

pub struct Common {
    pub collector: Collector,
    pub config: Config,
    pub metadata_cmd: MetadataCommand,
    color: ColorMode,
    short: bool,
    check: bool,
    html: Option<Utf8PathBuf>,
    excel: Option<Utf8PathBuf>,
    #[expect(dead_code, reason = "Stored for potential future use")]
    log_level: LogLevel,
}

impl Common {
    /// Create a new Common processor with logger, collector, and config
    ///
    /// # Errors
    ///
    /// Returns an error if the collector or config cannot be initialized
    pub async fn new(args: &CommonArgs) -> Result<Self> {
        Self::init_logging(args.log_level);

        // Create metadata command for workspace operations
        let mut metadata_cmd = MetadataCommand::new();
        let _ = metadata_cmd.manifest_path(&args.manifest_path);

        // Execute metadata command once and use it for both cache and config paths
        let metadata = metadata_cmd.exec().into_app_err("unable to retrieve workspace metadata")?;

        // Use workspace_root for config base path
        let config_base_path = metadata.workspace_root;

        // Load config from the determined base path first (we need the cache TTL)
        let (config, warnings) = Config::load(&config_base_path, args.config.as_ref())?;

        // Determine cache directory: use provided path or default cache directory for the platform
        let cache_dir = if let Some(cache_path) = &args.cache_dir {
            cache_path.as_std_path().to_path_buf()
        } else {
            BaseDirs::new()
                .into_app_err("Failed to determine cache directory")?
                .cache_dir()
                .join("cargo-rank")
        };

        // Create progress reporter
        // When logging is disabled, use a short delay so the progress bar appears for long operations
        // When logging is enabled, use an infinite delay so the progress bar never appears (would interfere with log output)
        let delay = if args.log_level == LogLevel::None {
            Duration::from_millis(500)
        } else {
            Duration::MAX
        };

        let progress_reporter = ProgressReporter::new(delay);

        let collector = Collector::new(
            args.github_token.as_deref(),
            &cache_dir,
            Duration::from_secs(config.crates_cache_ttl * 24 * 60 * 60),
            Duration::from_secs(config.hosting_cache_ttl * 24 * 60 * 60),
            Duration::from_secs(config.source_code_cache_ttl * 24 * 60 * 60),
            Duration::from_secs(config.coverage_cache_ttl * 24 * 60 * 60),
            Duration::from_secs(config.advisories_cache_ttl * 24 * 60 * 60),
            progress_reporter,
        )
        .await?;

        // Print warnings if any
        if !warnings.is_empty() {
            eprintln!("\n⚠️  Configuration validation warnings:");
            for warning in &warnings {
                eprintln!("   {warning}");
            }
            eprintln!();
        }

        // Create a fresh metadata command for the caller to use
        let mut metadata_cmd = MetadataCommand::new();
        let _ = metadata_cmd.manifest_path(&args.manifest_path);

        Ok(Self {
            collector,
            config,
            metadata_cmd,
            color: args.color,
            short: args.short,
            check: args.check,
            html: args.html.clone(),
            excel: args.excel.clone(),
            log_level: args.log_level,
        })
    }

    /// Initialize logger based on log level
    fn init_logging(log_level: LogLevel) {
        if log_level == LogLevel::None {
            return;
        }

        let level = match log_level {
            LogLevel::None => return, // Already checked above, but being explicit
            LogLevel::Error => "error",
            LogLevel::Warn => "warn",
            LogLevel::Info => "info",
            LogLevel::Debug => "debug",
            LogLevel::Trace => "trace",
        };

        let env = env_logger::Env::default().filter_or("RUST_LOG", level);

        env_logger::Builder::from_env(env)
            .format_timestamp(None)
            .format_module_path(false)
            .format_target(matches!(log_level, LogLevel::Debug) || matches!(log_level, LogLevel::Trace))
            .init();
    }

    pub async fn process_crates(&self, crates: impl IntoIterator<Item = CrateRef>) -> Result<Vec<CrateFacts>> {
        // Start background visibility checking for the progress bar
        let _visibility_guard = self.collector.progress().start_visibility_checking();

        // Use batch collection for efficiency
        let results = self.collector.collect(crates).await;

        match results {
            Ok(facts_iter) => Ok(facts_iter.map(|(_, facts)| facts).collect()),
            Err(e) => {
                eprintln!("{e}");
                Err(e)
            }
        }
    }

    pub fn report(&self, processed_crates: impl IntoIterator<Item = (CrateFacts, DependencyType)>) -> Result<()> {
        // Filter out crates with missing core data (can't be ranked/reported)
        let (analyzable_crates, failed_crates): (Vec<_>, Vec<_>) = processed_crates
            .into_iter()
            .partition(|(facts, _)| facts.crate_version_data.is_found() && facts.crate_overall_data.is_found());

        // Log crates that couldn't be analyzed
        if !failed_crates.is_empty() {
            eprintln!("\n⚠️  Unable to analyze {} crate(s) due to missing data:", failed_crates.len());
            for (facts, _) in &failed_crates {
                if let (ProviderResult::Found(version_data), ProviderResult::Found(overall_data)) =
                    (&facts.crate_version_data, &facts.crate_overall_data)
                {
                    eprintln!("  - {}@{}", overall_data.name, version_data.version);
                } else {
                    eprintln!("  - (unknown crate)");
                }
            }
            eprintln!();
        }

        let ranker = Ranker::new(&self.config);
        let scored_crates: Vec<_> = analyzable_crates
            .into_iter()
            .map(|(cd, dep_type)| {
                let sd = ranker.rank(&cd, dep_type);
                (cd, sd)
            })
            .collect();

        let generating_reports = self.html.is_some() || self.excel.is_some();

        if !generating_reports && !scored_crates.is_empty() {
            let mut console_output = String::new();
            _ = generate_console(&scored_crates, &self.config, self.color, self.short, &mut console_output);
            print!("{console_output}");
        }

        if let Some(filename) = &self.html {
            let mut html = String::new();
            generate_html(&scored_crates, &self.config, filename.as_str(), &mut html)?;
            fs::write(filename, html)?;
        }

        if let Some(filename) = &self.excel {
            let mut file = fs::File::create(filename)?;
            generate_xlsx(&scored_crates, &self.config, &mut file)?;
        }

        // Check if any crates are in the bad quality band if check flag is set
        if self.check {
            self.check_quality_gate(scored_crates.iter(), scored_crates.len())?;
        }

        Ok(())
    }

    #[expect(single_use_lifetimes, reason = "Required by Rust 2024 for impl Trait with references")]
    fn check_quality_gate<'a>(
        &self,
        scored_crates: impl Iterator<Item = &'a (CrateFacts, RankingOutcome)>,
        total_count: usize,
    ) -> Result<()> {
        let bad_crates: Vec<_> = scored_crates
            .filter_map(|(facts, ranking)| {
                let score = ranking.overall_score;
                // Check if the crate is in the "bad" band (index 0)
                // We can safely unwrap here because scored_crates only contain analyzable crates
                matches!(self.config.color_index_for_score(score), Some(0)).then(|| {
                    let ProviderResult::Found(ref overall) = facts.crate_overall_data else {
                        unreachable!("analyzable crate must have Found data");
                    };
                    (overall.name.as_str(), score)
                })
            })
            .collect();

        if bad_crates.is_empty() {
            println!("\n✓ Quality Check: All {total_count} crate(s) meet minimum quality standards");
            Ok(())
        } else {
            eprintln!("\n✗ Quality Check: {} crate(s) are in the 'bad' quality band:", bad_crates.len());
            for (name, score) in &bad_crates {
                eprintln!("  - {name} (score: {score:.1})");
            }

            bail!("quality check failed: {} crate(s) are in the 'bad' quality band", bad_crates.len())
        }
    }
}
