//! Rust edition type.

use serde::{Deserialize, Serialize};

/// Rust Edition.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RustEdition {
    #[serde(rename = "2015")]
    Edition2015,

    #[serde(rename = "2018")]
    Edition2018,

    #[serde(rename = "2021")]
    Edition2021,

    #[serde(rename = "2024")]
    Edition2024,

    #[serde(rename = "unknown")]
    Unknown,
}

impl RustEdition {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Edition2015 => "2015",
            Self::Edition2018 => "2018",
            Self::Edition2021 => "2021",
            Self::Edition2024 => "2024",
            Self::Unknown => "unknown",
        }
    }
}
