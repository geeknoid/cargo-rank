use camino::Utf8PathBuf;
use cargo_rank::Result;
use cargo_rank::config::Config;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct ConvertArgs {
    /// Input configuration file path
    #[arg(value_name = "INPUT")]
    pub input: Utf8PathBuf,

    /// Output configuration file path
    #[arg(value_name = "OUTPUT")]
    pub output: Utf8PathBuf,
}

pub fn convert_config(args: &ConvertArgs) -> Result<()> {
    let workspace_root = args.input.parent().unwrap_or(&args.input);

    match Config::load(workspace_root, Some(&args.input)) {
        Ok((config, warnings)) => {
            // Print warnings if any
            if !warnings.is_empty() {
                eprintln!("⚠️  Configuration validation warnings:");
                for warning in &warnings {
                    eprintln!("   {warning}");
                }
                eprintln!();
            }

            config.save(&args.output)?;
            println!("Converted {} to {}", args.input, args.output);
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to read facts configuration: {e}");
            std::process::exit(1);
        }
    }
}
