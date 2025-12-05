//! Owner kind type.

use serde::{Deserialize, Serialize};

/// The kind of owner (user or team).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum OwnerKind {
    User,
    Team,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_variant() {
        let kind = OwnerKind::User;
        assert_eq!(kind, OwnerKind::User);
    }

    #[test]
    fn test_team_variant() {
        let kind = OwnerKind::Team;
        assert_eq!(kind, OwnerKind::Team);
    }

    #[test]
    fn test_copy() {
        let kind1 = OwnerKind::User;
        let kind2 = kind1;

        // Both should be usable after copy
        assert_eq!(kind1, OwnerKind::User);
        assert_eq!(kind2, OwnerKind::User);
    }

    #[test]
    fn test_equality() {
        assert_eq!(OwnerKind::User, OwnerKind::User);
        assert_eq!(OwnerKind::Team, OwnerKind::Team);
        assert_ne!(OwnerKind::User, OwnerKind::Team);
    }

    #[test]
    fn test_serialize_user() {
        let kind = OwnerKind::User;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""user""#);
    }

    #[test]
    fn test_serialize_team() {
        let kind = OwnerKind::Team;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, r#""team""#);
    }

    #[test]
    fn test_deserialize_user() {
        let json = r#""user""#;
        let kind: OwnerKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, OwnerKind::User);
    }

    #[test]
    fn test_deserialize_team() {
        let json = r#""team""#;
        let kind: OwnerKind = serde_json::from_str(json).unwrap();
        assert_eq!(kind, OwnerKind::Team);
    }

    #[test]
    fn test_roundtrip_serialization() {
        let original = OwnerKind::User;
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: OwnerKind = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_hash() {
        use core::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let mut hasher1 = DefaultHasher::new();
        OwnerKind::User.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        OwnerKind::User.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        // Equal values should have equal hashes
        assert_eq!(hash1, hash2);
    }
}
