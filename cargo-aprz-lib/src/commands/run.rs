//! Command dispatch logic for cargo-aprz

use super::{CratesArgs, DepsArgs, InitArgs, ValidateArgs, init_config, process_crates, process_dependencies, validate_config};
use crate::{Host, Result};
use clap::builder::Styles;
use clap::builder::styling::{AnsiColor, Effects};
use clap::{Parser, Subcommand};

const CLAP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .usage(AnsiColor::Green.on_default().effects(Effects::BOLD))
    .literal(AnsiColor::Cyan.on_default().effects(Effects::BOLD))
    .placeholder(AnsiColor::Cyan.on_default());

#[derive(Parser, Debug)]
#[command(name = "cargo-aprz", bin_name = "cargo", version, about, author)]
#[command(styles = CLAP_STYLES)]
struct Cli {
    #[command(subcommand)]
    command: CargoSubcommand,
}

#[derive(Subcommand, Debug)]
enum CargoSubcommand {
    Aprz(Args),
}

#[derive(Parser, Debug)]
#[command(name = "cargo-aprz", author, version, long_about = None, display_name = "cargo-aprz")]
#[command(about = "Appraise the quality of Rust dependencies")]
struct Args {
    #[command(subcommand)]
    command: AprzSubcommand,
}

#[derive(Subcommand, Debug)]
enum AprzSubcommand {
    /// Analyze specific crates and generate quality reports
    Crates(Box<CratesArgs>),
    /// Analyze workspace dependencies and generate quality reports
    Deps(Box<DepsArgs>),
    /// Generate a default configuration file
    Init(InitArgs),
    /// Validate a configuration file
    Validate(ValidateArgs),
}

/// Dispatch command-line arguments to the appropriate handler
///
/// This function parses the command-line arguments and executes the corresponding
/// subcommand. It's designed to be called from main.rs with the program arguments.
///
/// # Arguments
///
/// * `args` - An iterator of command-line arguments (typically from `std::env::args()`)
///
/// # Errors
///
/// Returns an error if command parsing fails or if the executed command fails
pub async fn run<I, T, H>(host: &mut H, args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
    H: Host,
{
    let CargoSubcommand::Aprz(args) = Cli::parse_from(args).command;

    match &args.command {
        AprzSubcommand::Crates(crates_args) => process_crates(host, crates_args).await,
        AprzSubcommand::Deps(deps_args) => process_dependencies(host, deps_args).await,
        AprzSubcommand::Init(init_args) => init_config(host, init_args),
        AprzSubcommand::Validate(validate_args) => validate_config(host, validate_args),
    }
}
