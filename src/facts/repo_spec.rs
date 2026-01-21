use crate::Result;
use core::fmt::{Display, Formatter};
use ohno::bail;
use url::Url;

#[derive(Debug, PartialEq, Eq, Hash)]
pub struct RepoSpec {
    url: Url,
    host: Box<str>,
    owner: Box<str>,
    repo: Box<str>,
}

impl RepoSpec {
    pub fn parse(url: Url) -> Result<Self> {
        if url.host_str() != Some("github.com") {
            bail!("not a GitHub URL: {url}");
        }

        let path_segments: Vec<_> = url.path_segments().map(Iterator::collect).unwrap_or_default();

        if path_segments.len() < 2 {
            bail!("invalid repository URL format: {url}");
        }

        if path_segments[0].is_empty() || path_segments[1].is_empty() {
            bail!("invalid repository URL: empty owner or repo name: {url}");
        }

        Ok(Self {
            host: Box::from(url.host_str().unwrap_or_default()),
            owner: Box::from(path_segments[0]),
            repo: Box::from(path_segments[1].trim_end_matches(".git")),
            url,
        })
    }

    #[must_use]
    pub const fn url(&self) -> &Url {
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
