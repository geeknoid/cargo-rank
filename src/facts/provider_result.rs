use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(bound(serialize = "T: Clone + Serialize", deserialize = "T: Clone + for<'de2> Deserialize<'de2>"))]
pub enum ProviderResult<T> {
    /// The operation succeeded and data was found.
    Found(T),

    /// The requested crate name was not found.
    CrateNotFound,

    /// The crate exists but the requested version was not found.
    VersionNotFound,

    /// An error occurred during the operation for this crate.
    /// The error message is serialized as a string.
    #[serde(serialize_with = "serialize_error", deserialize_with = "deserialize_error")]
    Error(Arc<ohno::AppError>),
}

/// Serialize Arc<ohno::AppError> as a string
fn serialize_error<S>(error: &Arc<ohno::AppError>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&format!("{error}"))
}

/// Deserialize a string back into Arc<ohno::AppError>
fn deserialize_error<'de, D>(deserializer: D) -> Result<Arc<ohno::AppError>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let error_str = String::deserialize(deserializer)?;
    Ok(Arc::new(ohno::app_err!("{error_str}")))
}

impl<T: Clone> ProviderResult<T> {
    /// Returns `true` if the result is `Found`.
    #[must_use]
    pub const fn is_found(&self) -> bool {
        matches!(self, Self::Found(_))
    }

    /// Converts this result into a standard `Result`, mapping all non-Found variants to errors.
    ///
    /// # Errors
    ///
    /// Returns an error if the result is not `Found`.
    pub fn into_result(self) -> crate::Result<T> {
        match self {
            Self::Found(data) => Ok(data),
            Self::CrateNotFound => Err(ohno::app_err!("crate not found")),
            Self::VersionNotFound => Err(ohno::app_err!("version not found")),
            Self::Error(e) => Err(ohno::app_err!("{e}")),
        }
    }

    /// Converts this result into an `Option`, returning `Some` only for `Found`.
    #[must_use]
    pub fn ok(self) -> Option<T> {
        match self {
            Self::Found(data) => Some(data),
            _ => None,
        }
    }

    /// Returns a string describing the status of this result.
    #[must_use]
    pub const fn status_str(&self) -> &'static str {
        match self {
            Self::Found(_) => "Found",
            Self::CrateNotFound => "CrateNotFound",
            Self::VersionNotFound => "VersionNotFound",
            Self::Error(_) => "Error",
        }
    }
}
