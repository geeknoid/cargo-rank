use crate::misc::dependency_type::DependencyType;
use core::fmt::{Display, Formatter, Result};
use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyTypes(HashSet<DependencyType>);

impl DependencyTypes {
    #[must_use]
    pub fn contains(&self, dep_type: DependencyType) -> bool {
        self.0.contains(&dep_type)
    }

    pub fn iter(&self) -> impl Iterator<Item = &DependencyType> {
        self.0.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn intersect(&self, other: &Self) -> Self {
        Self(self.0.intersection(&other.0).copied().collect())
    }
}

impl Default for DependencyTypes {
    fn default() -> Self {
        let mut set = HashSet::with_capacity(1);
        let _ = set.insert(DependencyType::Standard);
        Self(set)
    }
}

impl From<Vec<DependencyType>> for DependencyTypes {
    fn from(vec: Vec<DependencyType>) -> Self {
        Self(vec.into_iter().collect())
    }
}

impl Display for DependencyTypes {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        if self.is_empty() {
            return write!(f, "(no types)");
        }

        let mut names: Vec<_> = self.iter().map(ToString::to_string).collect();
        names.sort_unstable();
        write!(f, "{}", names.join(", "))
    }
}

impl Serialize for DependencyTypes {
    fn serialize<S>(&self, serializer: S) -> core::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Sort for consistent output
        let mut names: Vec<_> = self.iter().map(ToString::to_string).collect();
        names.sort_unstable();
        serializer.serialize_str(&names.join(", "))
    }
}

impl<'de> Deserialize<'de> for DependencyTypes {
    fn deserialize<D>(deserializer: D) -> core::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DependencyTypesVisitor;

        impl Visitor<'_> for DependencyTypesVisitor {
            type Value = DependencyTypes;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> Result {
                formatter.write_str("a comma-separated string of dependency types")
            }

            fn visit_str<E>(self, v: &str) -> core::result::Result<Self::Value, E>
            where
                E: de::Error,
            {
                let mut set = HashSet::new();
                for part in v.split(',') {
                    let trimmed = part.trim();
                    if !trimmed.is_empty() {
                        let dep_type = trimmed
                            .parse::<DependencyType>()
                            .map_err(|_err| E::custom(format!("invalid dependency type: {trimmed}")))?;
                        let _ = set.insert(dep_type);
                    }
                }
                Ok(DependencyTypes(set))
            }
        }

        deserializer.deserialize_str(DependencyTypesVisitor)
    }
}
