use super::Host;
use super::config::Config;
use crate::Result;
use camino::Utf8PathBuf;
use cargo_metadata::MetadataCommand;
use clap::Parser;
use ohno::IntoAppError;
use std::io::Write;

#[derive(Parser, Debug)]
pub struct InitArgs {
    /// Output configuration file path (default is `aprz.toml` in workspace root)
    #[arg(value_name = "PATH")]
    pub output: Option<Utf8PathBuf>,

    /// Path to Cargo.toml file
    #[arg(long, default_value = "Cargo.toml", value_name = "PATH")]
    pub manifest_path: Utf8PathBuf,
}

pub fn init_config<H: Host>(host: &mut H, args: &InitArgs) -> Result<()> {
    let output = if let Some(path) = &args.output {
        path.clone()
    } else {
        let mut metadata_cmd = MetadataCommand::new();
        let _ = metadata_cmd.manifest_path(&args.manifest_path);
        let metadata = metadata_cmd.exec().into_app_err("unable to retrieve workspace metadata")?;
        metadata.workspace_root.join("aprz.toml")
    };

    Config::save_default(&output)?;
    let _ = writeln!(host.output(), "Generated default configuration file: {output}");
    Ok(())
}
