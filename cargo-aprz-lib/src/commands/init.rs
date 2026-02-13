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
        let metadata = metadata_cmd.exec().into_app_err("retrieving workspace metadata")?;
        metadata.workspace_root.join("aprz.toml")
    };

    Config::save_default(&output)?;
    let _ = writeln!(host.output(), "Generated default configuration file: {output}");
    Ok(())
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use std::io::Cursor;

    struct TestHost {
        output_buf: Vec<u8>,
        error_buf: Vec<u8>,
    }

    impl TestHost {
        fn new() -> Self {
            Self {
                output_buf: Vec::new(),
                error_buf: Vec::new(),
            }
        }
    }

    impl Host for TestHost {
        fn output(&mut self) -> impl Write {
            Cursor::new(&mut self.output_buf)
        }

        fn error(&mut self) -> impl Write {
            Cursor::new(&mut self.error_buf)
        }

        fn exit(&mut self, _code: i32) {}
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetModuleFileNameW")]
    fn test_init_config_default_output_path() {
        let mut host = TestHost::new();
        let args = InitArgs {
            output: None,
            manifest_path: Utf8PathBuf::from("Cargo.toml"),
        };

        // This exercises the else branch (line 25) where MetadataCommand
        // resolves the workspace root and appends "aprz.toml".
        let metadata = MetadataCommand::new().exec().expect("metadata");
        let generated = metadata.workspace_root.join("aprz.toml");
        let had_existing = generated.as_std_path().exists();
        let existing_contents = had_existing
            .then(|| std::fs::read(generated.as_std_path()).expect("read existing aprz.toml"));

        let result = init_config(&mut host, &args);
        assert!(result.is_ok(), "init_config should succeed: {result:?}");

        let output_text = String::from_utf8_lossy(&host.output_buf);
        assert!(
            output_text.contains("aprz.toml"),
            "output should mention aprz.toml, got: {output_text}"
        );

        // Restore original file or clean up the generated one
        if let Some(contents) = existing_contents {
            std::fs::write(generated.as_std_path(), contents).expect("restore aprz.toml");
        } else {
            let _ = std::fs::remove_file(generated.as_std_path());
        }
    }
}
