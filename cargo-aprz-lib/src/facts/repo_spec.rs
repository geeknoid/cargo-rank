use crate::Result;
use core::fmt::{Display, Formatter};
use ohno::{IntoAppError, bail};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RepoSpec {
    url: Arc<Url>,
    host: Arc<str>,
    owner: Arc<str>,
    repo: Arc<str>,
}

impl RepoSpec {
    pub fn parse(url: &Url) -> Result<Self> {
        let path_segments: Vec<_> = url.path_segments().map(Iterator::collect).unwrap_or_default();

        if path_segments.len() < 2 {
            bail!("invalid repository URL format: {url}");
        }

        if path_segments[0].is_empty() || path_segments[1].is_empty() {
            bail!("invalid repository URL: empty owner or repo name: {url}");
        }

        let host = url.host_str().unwrap_or_default();
        let owner = path_segments[0];
        let repo = path_segments[1].trim_end_matches(".git");
        let scheme = url.scheme();

        // Reconstruct a clean URL with only scheme://host/owner/repo
        let clean_url = Url::parse(&format!("{scheme}://{host}/{owner}/{repo}"))
            .into_app_err("reconstructing repository URL")?;

        Ok(Self {
            host: Arc::from(host),
            owner: Arc::from(owner),
            repo: Arc::from(repo),
            url: Arc::new(clean_url),
        })
    }

    #[must_use]
    pub fn url(&self) -> &Url {
        &self.url
    }

    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub fn owner(&self) -> &str {
        &self.owner
    }

    #[must_use]
    pub fn repo(&self) -> &str {
        &self.repo
    }
}

impl Display for RepoSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_url() {
        let url = Url::parse("https://github.com/tokio-rs/tokio").unwrap();
        let spec = RepoSpec::parse(&url).unwrap();

        assert_eq!(spec.host(), "github.com");
        assert_eq!(spec.owner(), "tokio-rs");
        assert_eq!(spec.repo(), "tokio");
        assert_eq!(spec.url().as_str(), "https://github.com/tokio-rs/tokio");
    }

    #[test]
    fn test_parse_codeberg_url() {
        let url = Url::parse("https://codeberg.org/msrd0/cargo-doc2readme").unwrap();
        let spec = RepoSpec::parse(&url).unwrap();

        assert_eq!(spec.host(), "codeberg.org");
        assert_eq!(spec.owner(), "msrd0");
        assert_eq!(spec.repo(), "cargo-doc2readme");
    }

    #[test]
    fn test_parse_url_with_git_extension() {
        let url = Url::parse("https://github.com/serde-rs/serde.git").unwrap();
        let spec = RepoSpec::parse(&url).unwrap();

        assert_eq!(spec.host(), "github.com");
        assert_eq!(spec.owner(), "serde-rs");
        assert_eq!(spec.repo(), "serde"); // .git should be stripped
    }

    #[test]
    fn test_parse_url_with_additional_path_segments() {
        let url = Url::parse("https://github.com/tokio-rs/tokio/tree/master").unwrap();
        let spec = RepoSpec::parse(&url).unwrap();

        assert_eq!(spec.owner(), "tokio-rs");
        assert_eq!(spec.repo(), "tokio");
        assert_eq!(spec.url().as_str(), "https://github.com/tokio-rs/tokio");
    }

    #[test]
    fn test_parse_url_with_deep_path_strips_to_repo() {
        let url = Url::parse("https://github.com/tokio-rs/tokio/tree/master/tokio-util").unwrap();
        let spec = RepoSpec::parse(&url).unwrap();

        assert_eq!(spec.owner(), "tokio-rs");
        assert_eq!(spec.repo(), "tokio");
        assert_eq!(spec.url().as_str(), "https://github.com/tokio-rs/tokio");
    }

    #[test]
    fn test_same_repo_different_paths_are_equal() {
        let url1 = Url::parse("https://github.com/tokio-rs/tokio/tree/master/tokio").unwrap();
        let url2 = Url::parse("https://github.com/tokio-rs/tokio/tree/master/tokio-util").unwrap();
        let spec1 = RepoSpec::parse(&url1).unwrap();
        let spec2 = RepoSpec::parse(&url2).unwrap();

        assert_eq!(spec1, spec2);
    }

    #[test]
    fn test_same_repo_with_and_without_path_are_equal() {
        let url1 = Url::parse("https://github.com/tokio-rs/tokio").unwrap();
        let url2 = Url::parse("https://github.com/tokio-rs/tokio/tree/master/tokio-util").unwrap();
        let spec1 = RepoSpec::parse(&url1).unwrap();
        let spec2 = RepoSpec::parse(&url2).unwrap();

        assert_eq!(spec1, spec2);
    }

    #[test]
    fn test_git_extension_with_path_stripped() {
        let url = Url::parse("https://github.com/serde-rs/serde.git/tree/master/serde_derive").unwrap();
        let spec = RepoSpec::parse(&url).unwrap();

        assert_eq!(spec.owner(), "serde-rs");
        assert_eq!(spec.repo(), "serde");
        assert_eq!(spec.url().as_str(), "https://github.com/serde-rs/serde");
    }

    #[test]
    fn test_parse_invalid_url_missing_segments() {
        let url = Url::parse("https://github.com/").unwrap();
        let _ = RepoSpec::parse(&url).unwrap_err();
    }

    #[test]
    fn test_parse_invalid_url_only_owner() {
        let url = Url::parse("https://github.com/tokio-rs").unwrap();
        let _ = RepoSpec::parse(&url).unwrap_err();
    }

    #[test]
    fn test_parse_invalid_url_empty_owner() {
        let url = Url::parse("https://github.com//tokio").unwrap();
        let _ = RepoSpec::parse(&url).unwrap_err();
    }

    #[test]
    fn test_parse_invalid_url_empty_repo() {
        let url = Url::parse("https://github.com/tokio-rs/").unwrap();
        let _ = RepoSpec::parse(&url).unwrap_err();
    }

    #[test]
    fn test_display_trait() {
        let url = Url::parse("https://github.com/tokio-rs/tokio").unwrap();
        let spec = RepoSpec::parse(&url).unwrap();

        assert_eq!(spec.to_string(), "https://github.com/tokio-rs/tokio");
    }

    #[test]
    fn test_clone_and_equality() {
        let url = Url::parse("https://github.com/tokio-rs/tokio").unwrap();
        let spec1 = RepoSpec::parse(&url).unwrap();
        let spec2 = spec1.clone();

        assert_eq!(spec1, spec2);
    }
}
