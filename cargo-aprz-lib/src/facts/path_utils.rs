//! Path utilities for safe filesystem operations.

/// Sanitize a string for use as a path component
///
/// Removes path traversal sequences and dangerous characters to prevent
/// directory traversal attacks and filesystem issues.
///
/// # Examples
///
/// ```ignore
/// // This is an internal utility function
/// assert_eq!(sanitize_path_component("normal-name"), "normal-name");
/// assert_eq!(sanitize_path_component("../../etc/passwd"), "______etc_passwd");
/// assert_eq!(sanitize_path_component("my/dangerous:file?"), "my_dangerous_file_");
/// ```
#[must_use]
pub fn sanitize_path_component(s: &str) -> String {
    // First remove path traversal sequences (replace ".." but allow single ".")
    // This preserves crate names like "my.crate" while preventing "../" attacks
    let s = s.replace("..", "__");
    // Then remove other dangerous filesystem characters
    s.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_normal_name() {
        assert_eq!(sanitize_path_component("tokio"), "tokio");
        assert_eq!(sanitize_path_component("my-crate"), "my-crate");
        assert_eq!(sanitize_path_component("my.crate"), "my.crate");
    }

    #[test]
    fn test_sanitize_path_traversal() {
        assert_eq!(sanitize_path_component(".."), "__");
        assert_eq!(sanitize_path_component("../etc"), "___etc");
        assert_eq!(sanitize_path_component("../../etc/passwd"), "______etc_passwd");
    }

    #[test]
    fn test_sanitize_dangerous_chars() {
        assert_eq!(sanitize_path_component("foo/bar"), "foo_bar");
        assert_eq!(sanitize_path_component("foo\\bar"), "foo_bar");
        assert_eq!(sanitize_path_component("foo:bar"), "foo_bar");
        assert_eq!(sanitize_path_component("foo*bar"), "foo_bar");
        assert_eq!(sanitize_path_component("foo?bar"), "foo_bar");
        assert_eq!(sanitize_path_component("foo\"bar"), "foo_bar");
        assert_eq!(sanitize_path_component("foo<bar"), "foo_bar");
        assert_eq!(sanitize_path_component("foo>bar"), "foo_bar");
        assert_eq!(sanitize_path_component("foo|bar"), "foo_bar");
    }

    #[test]
    fn test_sanitize_combined() {
        assert_eq!(sanitize_path_component("../dangerous:file?.txt"), "___dangerous_file_.txt");
    }
}
