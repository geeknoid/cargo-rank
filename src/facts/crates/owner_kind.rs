//! Owner kind type.

use serde::{Deserialize, Serialize};

/// The kind of owner (user or team).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum OwnerKind {
    User,
    Team,
}
