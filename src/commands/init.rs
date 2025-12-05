use camino::Utf8PathBuf;
use cargo_rank::Result;
use cargo_rank::config::Config;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct InitArgs {
    /// Output configuration file path
    #[arg(value_name = "PATH", default_value = "rank.toml")]
    pub output: Utf8PathBuf,
}

pub fn init_config(args: &InitArgs) -> Result<()> {
    let config = Config::default();
    config.save_default_with_comments(&args.output)?;
    println!("Generated default configuration file: {}", args.output);
    Ok(())
}
