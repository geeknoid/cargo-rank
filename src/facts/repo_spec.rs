use crate::Result;
use core::fmt::{Display, Formatter};
use ohno::bail;
use std::sync::Arc;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepoSpec {
    url: Arc<Url>,
    host: Arc<str>,
    owner: Arc<str>,
    repo: Arc<str>,
}

impl RepoSpec {
    pub fn parse(url: Url) -> Result<Self> {
        let path_segments: Vec<_> = url.path_segments().map(Iterator::collect).unwrap_or_default();

        if path_segments.len() < 2 {
            bail!("invalid repository URL format: {url}");
        }

        if path_segments[0].is_empty() || path_segments[1].is_empty() {
            bail!("invalid repository URL: empty owner or repo name: {url}");
        }

        Ok(Self {
            host: Arc::from(url.host_str().unwrap_or_default()),
            owner: Arc::from(path_segments[0]),
            repo: Arc::from(path_segments[1].trim_end_matches(".git")),
            url: Arc::new(url),
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
