use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, ValueEnum, Deserialize, Serialize, Display, EnumString)]
#[value(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum DependencyType {
    Standard,
    Dev,
    Build,
}
