//! Owner type.

use super::owner_kind::OwnerKind;
use compact_str::CompactString;
use serde::{Deserialize, Serialize};

/// A crate owner (can be a user or team).
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Owner {
    /// The login name of the team or user.
    pub login: CompactString,

    /// The kind of the owner (`user` or `team`).
    pub kind: OwnerKind,

    /// The display name of the team or user.
    pub name: Option<CompactString>,
}
