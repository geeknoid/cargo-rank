use super::CoverageData;
use crate::Result;
use crate::facts::ProviderResult;
use crate::facts::cache_doc;
use crate::facts::crate_spec::{self, CrateSpec};
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::repo_spec::RepoSpec;
use crate::facts::request_tracker::RequestTracker;
use chrono::Utc;
use core::time::Duration;
use futures_util::future::join_all;
use ohno::EnrichableExt;
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

const LOG_TARGET: &str = "  coverage";

static PERCENT_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)%").expect("invalid regex"));

#[derive(Debug, Clone)]
pub struct Provider {
    client: Arc<reqwest::Client>,
    cache_dir: Arc<Path>,
    cache_ttl: Duration,
}

impl Provider {
    #[must_use]
    pub fn new(cache_dir: impl AsRef<Path>, cache_ttl: Duration) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("cargo-rank")
            .build()
            .expect("unable to create HTTP client");

        Self {
            client: Arc::new(client),
            cache_dir: Arc::from(cache_dir.as_ref()),
            cache_ttl,
        }
    }

    pub async fn get_coverage_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
        tracker: &RequestTracker,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<CoverageData>)> {
        let mut repo_to_crates = crate_spec::by_repo(crates);

        tracker.add_many_requests("coverage", repo_to_crates.len() as u64);

        join_all(repo_to_crates.keys().map(|repo_spec| {
            let repo_spec = Arc::clone(repo_spec);
            let tracker = tracker.clone();
            self.fetch_coverage_data_for_repo(repo_spec, tracker)
        }))
        .await
        .into_iter()
        .flat_map(move |(repo_spec, provider_result)| {
            let crate_specs = repo_to_crates.remove(&repo_spec).expect("repo_spec must exist");
            crate_specs.into_iter().map(move |crate_spec| (crate_spec, provider_result.clone()))
        })
    }

    /// Get code coverage data for a single repository
    async fn fetch_coverage_data_for_repo(
        &self,
        repo_spec: Arc<RepoSpec>,
        tracker: RequestTracker,
    ) -> (Arc<RepoSpec>, ProviderResult<CoverageData>) {
        let result = self.fetch_coverage_data_for_repo_core(&repo_spec).await;
        tracker.complete_request("coverage");

        (repo_spec, result)
    }

    async fn fetch_coverage_data_for_repo_core(&self, repo_spec: &RepoSpec) -> ProviderResult<CoverageData> {
        let cache_path = self.get_cache_path(repo_spec);

        if let Some(data) = cache_doc::load_with_ttl(
            &cache_path,
            self.cache_ttl,
            |data: &CoverageData| data.timestamp,
            format!("coverage for repository '{repo_spec}'"),
        ) {
            return ProviderResult::Found(data);
        }

        log::debug!(target: LOG_TARGET, "Fetching coverage data for repository '{repo_spec}'");

        let code_coverage_percentage = match get_code_coverage(&self.client, repo_spec).await {
            Ok(Some(coverage)) => coverage,
            Ok(None) => {
                log::debug!(target: LOG_TARGET, "No coverage data found for repository '{repo_spec}'");
                return ProviderResult::CrateNotFound;
            }
            Err(e) => {
                return ProviderResult::Error(Arc::new(
                    e.enrich_with(|| format!("could not fetch coverage for repository '{repo_spec}'")),
                ));
            }
        };

        let coverage_data = CoverageData {
            timestamp: Utc::now(),
            code_coverage_percentage,
        };

        log::debug!(target: LOG_TARGET, "Fetched coverage data for repository '{repo_spec}'");

        match cache_doc::save(&coverage_data, &cache_path) {
            Ok(()) => ProviderResult::Found(coverage_data),
            Err(e) => ProviderResult::Error(Arc::new(e)),
        }
    }

    /// Get the cache file path for a repository
    fn get_cache_path(&self, repo_spec: &RepoSpec) -> PathBuf {
        let safe_owner = sanitize_path_component(repo_spec.owner());
        let safe_repo = sanitize_path_component(repo_spec.repo());
        let filename = format!("{safe_repo}.json");
        self.cache_dir.join(safe_owner).join(filename)
    }
}

/// Try to fetch codecov badge for a specific branch
async fn try_branch(
    repo_spec: &RepoSpec,
    client: &reqwest::Client,
    owner: &str,
    repo: &str,
    branch: &str,
) -> Result<Option<reqwest::Response>> {
    let codecov_url = format!("https://codecov.io/gh/{owner}/{repo}/branch/{branch}/graph/badge.svg");
    log::info!(target: LOG_TARGET, "Querying codecov.io for coverage of repository '{}'", repo_spec.url());

    let response = client.get(&codecov_url).send().await.map_err(|e| {
        log::debug!(target: LOG_TARGET, "Could not send HTTP request to codecov.io ('{branch}' branch): {e}");
        e
    })?;

    let status = response.status();
    if status.is_success() { Ok(Some(response)) } else { Ok(None) }
}

/// Fetch codebase coverage data from codecov.io
pub async fn get_code_coverage(client: &reqwest::Client, repo_spec: &RepoSpec) -> Result<Option<f64>> {
    let owner = &repo_spec.owner();
    let repo = &repo_spec.repo();

    // Try main branch first, then master
    let response = try_branch(repo_spec, client, owner, repo, "main").await?;
    let response = if let Some(r) = response {
        r
    } else if let Some(r) = try_branch(repo_spec, client, owner, repo, "master").await? {
        r
    } else {
        log::info!(target: LOG_TARGET, "No codecov data available for repository '{repo_spec}'");
        return Ok(None);
    };

    let text = response.text().await.map_err(|e| {
        log::debug!(target: LOG_TARGET, "Could not read response body: {e}");
        e
    })?;

    log::debug!(target: LOG_TARGET, "Codecov SVG length: {} bytes", text.len());
    #[cfg(debug_assertions)]
    log::debug!(target: LOG_TARGET, "Full codecov SVG content:\n{}", &text);

    // Look for coverage percentage in the SVG
    // Codecov badges typically have the percentage in a <text> element
    // Example: <text>89%</text> or <text x="..." y="...">89%</text>
    if let Some(captures) = PERCENT_REGEX.captures(&text)
        && let Some(percent_str) = captures.get(1)
    {
        let percent_str = percent_str.as_str();

        let coverage = percent_str.parse::<f64>().map_err(|e| {
            log::debug!(target: LOG_TARGET, "Could not parse coverage percentage '{percent_str}'");
            ohno::app_err!("could not parse coverage percentage '{percent_str}': {e}")
        })?;

        return Ok(Some(coverage));
    }

    Ok(None)
}
