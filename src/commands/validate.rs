use camino::Utf8PathBuf;
use cargo_rank::Result;
use cargo_rank::config::Config;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct ValidateArgs {
    /// Path to configuration file [default: one of rank.[toml|yml|yaml|json] ]
    #[arg(long, short = 'c', value_name = "PATH")]
    pub config: Option<Utf8PathBuf>,
}

#[expect(clippy::unnecessary_wraps, reason = "Consistent interface with other subcommands")]
pub fn validate_config(args: &ValidateArgs) -> Result<()> {
    let workspace_root = Utf8PathBuf::from(".");
    let config_path = args.config.as_ref();

    match Config::load(&workspace_root, config_path) {
        Ok((_, warnings)) => {
            println!("Configuration validation successful");
            if let Some(path) = config_path {
                println!("Config file: {path}");
            } else {
                println!("Using default configuration (no config file found)");
            }

            // Print warnings if any
            if !warnings.is_empty() {
                eprintln!("\n⚠️  Configuration validation warnings:");
                for warning in &warnings {
                    eprintln!("   {warning}");
                }
                eprintln!();
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Configuration validation failed: {e}");
            std::process::exit(1);
        }
    }
}
