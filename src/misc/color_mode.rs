//! Color mode configuration for reports.

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ColorMode {
    Always,
    Never,
    Auto,
}
