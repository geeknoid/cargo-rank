//!! This build script validates the default configuration file (`default_config.yml`)

#![allow(
    clippy::redundant_pub_crate,
    reason = "pub(crate) is correct in library context but appears redundant in build script"
)]
#![allow(dead_code, reason = "Some items may be unused in this build script context")]
#![allow(unused_imports, reason = "Some items may be unused in this build script context")]

use ohno::{AppError, IntoAppError};

type Result<T, E = ohno::AppError> = core::result::Result<T, E>;
use camino::Utf8PathBuf;
use std::env;
use std::process;

#[path = "src/metrics/mod.rs"]
mod metrics;

#[path = "src/misc/mod.rs"]
mod misc;

#[path = "src/config/mod.rs"]
mod config;

fn main() {
    // Declare custom cfg flag for conditional compilation
    println!("cargo::rustc-check-cfg=cfg(all_fields,all_tables)");

    match inner_main() {
        Ok(warnings) => {
            if !warnings.is_empty() {
                for warning in warnings {
                    eprintln!("cargo:warning=Config validation warning: {warning}");
                }

                process::exit(1);
            }

            println!("cargo:rerun-if-changed=default_config.yml");
            println!("cargo:rerun-if-changed=src/config");
            process::exit(0);
        }
        Err(e) => {
            eprintln!("unable to load default_config.yml: {e:?}");
            process::exit(1);
        }
    }
}

fn inner_main() -> Result<Vec<String>> {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").into_app_err("CARGO_MANIFEST_DIR should be set during build")?;
    let workspace_root = Utf8PathBuf::from(&manifest_dir);
    let config_path = workspace_root.join("default_config.yml");

    let (_config, warnings) =
        config::Config::load(&workspace_root, Some(&config_path)).into_app_err("unable to load default_config.yml")?;

    Ok(warnings)
}
