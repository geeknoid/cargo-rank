use compact_str::CompactString;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum ProviderResult<T> {
    /// The operation succeeded and data was found.
    Found(T),

    /// The requested crate name was not found.
    CrateNotFound(Arc<[CompactString]>),

    /// The crate exists but the requested version was not found.
    VersionNotFound,

    /// An error occurred during the operation for this crate.
    Error(Arc<ohno::AppError>),
}

impl<T: Clone> ProviderResult<T> {
    /// Returns `true` if the result is `Found`.
    #[must_use]
    pub const fn is_found(&self) -> bool {
        matches!(self, Self::Found(_))
    }

    /// Returns a reference to the contained data if `Found`, otherwise `None`.
    #[must_use]
    pub const fn as_ref(&self) -> Option<&T> {
        match self {
            Self::Found(data) => Some(data),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ohno::app_err;

    #[test]
    fn test_is_found_for_found_variant() {
        let result: ProviderResult<String> = ProviderResult::Found("data".to_string());
        assert!(result.is_found());
    }

    #[test]
    fn test_is_found_for_crate_not_found() {
        let result: ProviderResult<String> = ProviderResult::CrateNotFound(Arc::from(vec!["similar1".into(), "similar2".into()]));
        assert!(!result.is_found());
    }

    #[test]
    fn test_is_found_for_version_not_found() {
        let result: ProviderResult<String> = ProviderResult::VersionNotFound;
        assert!(!result.is_found());
    }

    #[test]
    fn test_is_found_for_error() {
        let result: ProviderResult<String> = ProviderResult::Error(Arc::new(app_err!("test error")));
        assert!(!result.is_found());
    }

    #[test]
    fn test_as_ref_for_found() {
        let result: ProviderResult<u32> = ProviderResult::Found(42);
        assert_eq!(result.as_ref(), Some(&42));
    }

    #[test]
    fn test_as_ref_for_crate_not_found() {
        let result: ProviderResult<u32> = ProviderResult::CrateNotFound(Arc::from(vec![]));
        assert_eq!(result.as_ref(), None);
    }

    #[test]
    fn test_as_ref_for_version_not_found() {
        let result: ProviderResult<u32> = ProviderResult::VersionNotFound;
        assert_eq!(result.as_ref(), None);
    }

    #[test]
    fn test_as_ref_for_error() {
        let result: ProviderResult<u32> = ProviderResult::Error(Arc::new(app_err!("error")));
        assert_eq!(result.as_ref(), None);
    }

    #[test]
    fn test_clone_found() {
        let result: ProviderResult<String> = ProviderResult::Found("data".to_string());
        let cloned = result;

        assert!(cloned.is_found());
        assert_eq!(cloned.as_ref(), Some(&"data".to_string()));
    }

    #[test]
    fn test_clone_crate_not_found() {
        let suggestions: Arc<[CompactString]> = Arc::from(vec!["similar".into()]);
        let result: ProviderResult<String> = ProviderResult::CrateNotFound(suggestions);
        let cloned = result;

        assert!(!cloned.is_found());
        match cloned {
            ProviderResult::CrateNotFound(s) => assert_eq!(s.len(), 1),
            _ => panic!("Expected CrateNotFound"),
        }
    }

    #[test]
    fn test_clone_version_not_found() {
        let result: ProviderResult<String> = ProviderResult::VersionNotFound;
        let cloned = result;

        assert!(!cloned.is_found());
        assert!(matches!(cloned, ProviderResult::VersionNotFound));
    }

    #[test]
    fn test_clone_error() {
        let result: ProviderResult<String> = ProviderResult::Error(Arc::new(app_err!("test error")));
        let cloned = result;

        assert!(!cloned.is_found());
        assert!(matches!(cloned, ProviderResult::Error(_)));
    }

    #[test]
    fn test_debug_found() {
        let result: ProviderResult<i32> = ProviderResult::Found(42);
        let debug_str = format!("{result:?}");
        assert!(debug_str.contains("Found"));
        assert!(debug_str.contains("42"));
    }

    #[test]
    fn test_debug_crate_not_found() {
        let result: ProviderResult<i32> = ProviderResult::CrateNotFound(Arc::from(vec!["similar".into()]));
        let debug_str = format!("{result:?}");
        assert!(debug_str.contains("CrateNotFound"));
    }

    #[test]
    fn test_debug_version_not_found() {
        let result: ProviderResult<i32> = ProviderResult::VersionNotFound;
        let debug_str = format!("{result:?}");
        assert!(debug_str.contains("VersionNotFound"));
    }

    #[test]
    fn test_debug_error() {
        let result: ProviderResult<i32> = ProviderResult::Error(Arc::new(app_err!("test error")));
        let debug_str = format!("{result:?}");
        assert!(debug_str.contains("Error"));
    }

    #[test]
    fn test_found_with_complex_type() {
        #[derive(Debug, Clone, PartialEq)]
        struct TestData {
            value: String,
            count: u32,
        }

        let data = TestData {
            value: "test".to_string(),
            count: 10,
        };
        let result: ProviderResult<TestData> = ProviderResult::Found(data.clone());

        assert!(result.is_found());
        assert_eq!(result.as_ref(), Some(&data));
    }

    #[test]
    fn test_crate_not_found_empty_suggestions() {
        let result: ProviderResult<String> = ProviderResult::CrateNotFound(Arc::from(vec![]));
        assert!(!result.is_found());
        assert_eq!(result.as_ref(), None);
    }

    #[test]
    fn test_crate_not_found_multiple_suggestions() {
        let suggestions: Arc<[CompactString]> = Arc::from(vec!["similar1".into(), "similar2".into(), "similar3".into()]);
        let result: ProviderResult<String> = ProviderResult::CrateNotFound(suggestions);

        match result {
            ProviderResult::CrateNotFound(s) => assert_eq!(s.len(), 3),
            _ => panic!("Expected CrateNotFound"),
        }
    }

    #[test]
    fn test_pattern_matching() {
        let found: ProviderResult<i32> = ProviderResult::Found(42);
        match found {
            ProviderResult::Found(v) => assert_eq!(v, 42),
            _ => panic!("Expected Found"),
        }

        let not_found: ProviderResult<i32> = ProviderResult::CrateNotFound(Arc::from(vec![]));
        match not_found {
            ProviderResult::CrateNotFound(_) => {}
            _ => panic!("Expected CrateNotFound"),
        }

        let version_not_found: ProviderResult<i32> = ProviderResult::VersionNotFound;
        match version_not_found {
            ProviderResult::VersionNotFound => {}
            _ => panic!("Expected VersionNotFound"),
        }

        let error: ProviderResult<i32> = ProviderResult::Error(Arc::new(app_err!("error")));
        match error {
            ProviderResult::Error(_) => {}
            _ => panic!("Expected Error"),
        }
    }
}
