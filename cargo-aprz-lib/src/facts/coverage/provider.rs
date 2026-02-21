use super::CoverageData;
use crate::Result;
use crate::facts::ProviderResult;
use crate::facts::cache::{Cache, CacheResult};
use crate::facts::crate_spec::{self, CrateSpec};
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::repo_spec::RepoSpec;
use crate::facts::request_tracker::{RequestTracker, TrackedTopic};
use crate::facts::throttler::Throttler;
use futures_util::future::join_all;
use ohno::{EnrichableExt, IntoAppError};
use regex::Regex;
use std::sync::{Arc, LazyLock};

const LOG_TARGET: &str = "  coverage";

pub const CODECOV_BASE_URL: &str = "https://codecov.io";

static PERCENT_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)%").expect("invalid regex"));

const MAX_CONCURRENT_REQUESTS: usize = 5;

#[derive(Debug, Clone)]
pub struct Provider {
    client: Arc<reqwest::Client>,
    cache: Cache,
    base_url: String,
    throttler: Arc<Throttler>,
}

impl Provider {
    #[must_use]
    pub fn new(cache: Cache, base_url: Option<&str>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("cargo-aprz")
            .build()
            .expect("unable to create HTTP client");

        Self {
            client: Arc::new(client),
            cache,
            base_url: base_url.unwrap_or(CODECOV_BASE_URL).to_string(),
            throttler: Throttler::new(MAX_CONCURRENT_REQUESTS),
        }
    }

    pub async fn get_coverage_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
        tracker: &RequestTracker,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<CoverageData>)> {
        let mut repo_to_crates = crate_spec::by_repo(crates);

        tracker.add_requests(TrackedTopic::Coverage, repo_to_crates.len() as u64);

        join_all(repo_to_crates.keys().map(|repo_spec| {
            let repo_spec = repo_spec.clone();
            let tracker = tracker.clone();
            self.fetch_coverage_data_for_repo(repo_spec, tracker)
        }))
        .await
        .into_iter()
        .flat_map(move |(repo_spec, provider_result)| {
            let crate_specs = repo_to_crates.remove(&repo_spec).expect("repo_spec must exist");
            crate_specs.into_iter().map(move |crate_spec| (crate_spec, provider_result.clone()))
        })
        .inspect(|(crate_spec, result)| {
            if let ProviderResult::Error(e) = result {
                log::error!(target: LOG_TARGET, "Could not get code coverage data for {crate_spec}: {e:#}");
            } else if let ProviderResult::Unavailable(reason) = result {
                log::warn!(target: LOG_TARGET, "Coverage unavailable for {crate_spec}: {reason}");
            }
        })
    }

    /// Get code coverage data for a single repository
    async fn fetch_coverage_data_for_repo(&self, repo_spec: RepoSpec, tracker: RequestTracker) -> (RepoSpec, ProviderResult<CoverageData>) {
        let _permit = self.throttler.acquire().await;
        let result = self.fetch_coverage_data_for_repo_core(&repo_spec).await;
        tracker.complete_request(TrackedTopic::Coverage);

        (repo_spec, result)
    }

    async fn fetch_coverage_data_for_repo_core(&self, repo_spec: &RepoSpec) -> ProviderResult<CoverageData> {
        let filename = Self::get_cache_filename(repo_spec);

        match self.cache.load::<CoverageData>(&filename) {
            CacheResult::Data(data) => return ProviderResult::Found(data),
            CacheResult::NoData(reason) => return ProviderResult::Unavailable(reason.into()),
            CacheResult::Miss => {}
        }

        let code_coverage_percentage = match self.get_code_coverage(repo_spec).await {
            Ok(Some(coverage)) => coverage,
            Ok(None) => {
                let reason = format!("could not find coverage data for repository '{repo_spec}' on codecov.io");
                if let Err(e) = self.cache.save_no_data(&filename, &reason) {
                    log::debug!(target: LOG_TARGET, "Could not save cache for '{repo_spec}': {e:#}");
                    return ProviderResult::Error(Arc::new(e));
                }
                return ProviderResult::Unavailable(reason.into());
            }
            Err(e) => {
                return ProviderResult::Error(Arc::new(
                    e.enrich_with(|| format!("fetching coverage for repository '{repo_spec}'")),
                ));
            }
        };

        let coverage_data = CoverageData {
            code_coverage_percentage,
        };

        log::debug!(target: LOG_TARGET, "Fetched coverage data for repository '{repo_spec}'");

        match self.cache.save(&filename, &coverage_data) {
            Ok(()) => ProviderResult::Found(coverage_data),
            Err(e) => {
                log::debug!(target: LOG_TARGET, "Could not save cache for '{repo_spec}': {e:#}");
                ProviderResult::Error(Arc::new(e))
            }
        }
    }

    /// Get the cache filename for a repository
    fn get_cache_filename(repo_spec: &RepoSpec) -> String {
        let safe_host = sanitize_path_component(repo_spec.host());
        let safe_owner = sanitize_path_component(repo_spec.owner());
        let safe_repo = sanitize_path_component(repo_spec.repo());
        format!("{safe_host}/{safe_owner}/{safe_repo}.json")
    }

    /// Fetch codebase coverage data from codecov.io
    async fn get_code_coverage(&self, repo_spec: &RepoSpec) -> Result<Option<f64>> {
        log::info!(target: LOG_TARGET, "Querying '{}' for code coverage of repository '{repo_spec}'", self.base_url);

        for branch in &["main", "master"] {
            if let Some(coverage) = self.try_branch_coverage(repo_spec, branch).await? {
                return Ok(Some(coverage));
            }
        }

        Ok(None)
    }

    /// Try to fetch codecov badge for a specific branch and extract the coverage percentage.
    ///
    /// Returns `Ok(Some(percentage))` if coverage was found, `Ok(None)` if the branch
    /// has no coverage data (404, "unknown" badge, or unparseable SVG), or `Err` on
    /// network/transport failures.
    async fn try_branch_coverage(
        &self,
        repo_spec: &RepoSpec,
        branch: &str,
    ) -> Result<Option<f64>> {
        let owner = repo_spec.owner();
        let repo = repo_spec.repo();
        let codecov_url = format!("{}/gh/{owner}/{repo}/branch/{branch}/graph/badge.svg", self.base_url);
        log::debug!(target: LOG_TARGET, "Trying branch '{branch}' for repository '{repo_spec}'");

        let response = crate::facts::resilient_http::resilient_get(&self.client, &codecov_url)
            .await
            .into_app_err_with(|| format!("sending HTTP request to {codecov_url}"))?;

        let status = response.status();
        if status.is_client_error() {
            return Ok(None);
        }
        if !status.is_success() {
            return Err(ohno::app_err!("unexpected HTTP status {status} from {codecov_url}"));
        }

        let text = response
            .text()
            .await
            .into_app_err("reading codecov response body")?;

        log::debug!(target: LOG_TARGET, "Codecov SVG length: {} bytes", text.len());

        // codecov returns a 200 with "unknown" in the SVG when no coverage data is available
        if text.contains(">unknown<") {
            log::debug!(target: LOG_TARGET, "Codecov badge shows 'unknown' for branch '{branch}' - no coverage data available");
            return Ok(None);
        }

        // Look for coverage percentage in the SVG
        if let Some(captures) = PERCENT_REGEX.captures(&text)
            && let Some(percent_str) = captures.get(1)
        {
            let percent_str = percent_str.as_str();

            let coverage = match percent_str.parse::<f64>() {
                Ok(v) => v,
                Err(e) => {
                    log::debug!(target: LOG_TARGET, "Could not parse coverage percentage '{percent_str}': {e:#}");
                    return Ok(None);
                }
            };

            log::debug!(target: LOG_TARGET, "Found coverage: {coverage}%");
            return Ok(Some(coverage));
        }

        log::debug!(target: LOG_TARGET, "No percentage found in codecov SVG for branch '{branch}'");
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::RepoSpec;

    #[test]
    fn test_get_cache_filename() {
        let url = url::Url::parse("https://github.com/tokio-rs/tokio").unwrap();
        let repo_spec = RepoSpec::parse(&url).unwrap();
        let filename = Provider::get_cache_filename(&repo_spec);
        assert!(filename.contains("github.com"));
        assert!(filename.contains("tokio-rs"));
        assert!(filename.contains("tokio.json"));
    }

    #[test]
    fn test_get_cache_filename_sanitized() {
        let url = url::Url::parse("https://evil.com/../../etc/passwd").unwrap();
        let repo_spec = RepoSpec::parse(&url).unwrap();
        let filename = Provider::get_cache_filename(&repo_spec);
        assert!(!filename.contains("../"));
    }

    #[test]
    fn test_percent_regex_integer() {
        let text = "<text>85%</text>";
        let captures = PERCENT_REGEX.captures(text).unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "85");
    }

    #[test]
    fn test_percent_regex_decimal() {
        let text = "<text>93.4%</text>";
        let captures = PERCENT_REGEX.captures(text).unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "93.4");
    }

    #[test]
    fn test_percent_regex_no_match() {
        let text = "<text>unknown</text>";
        assert!(PERCENT_REGEX.captures(text).is_none());
    }

    #[test]
    fn test_percent_regex_zero() {
        let text = "<text>0%</text>";
        let captures = PERCENT_REGEX.captures(text).unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "0");
    }

    #[test]
    fn test_percent_regex_hundred() {
        let text = "<text>100%</text>";
        let captures = PERCENT_REGEX.captures(text).unwrap();
        assert_eq!(captures.get(1).unwrap().as_str(), "100");
    }

    #[test]
    fn test_provider_new_default_base_url() {
        let cache = Cache::new(
            "/tmp/test",
            core::time::Duration::from_secs(3600),
            chrono::Utc::now(),
            false,
        );
        let provider = Provider::new(cache, None);
        assert_eq!(provider.base_url, CODECOV_BASE_URL);
    }

    #[test]
    fn test_provider_new_custom_base_url() {
        let cache = Cache::new(
            "/tmp/test",
            core::time::Duration::from_secs(3600),
            chrono::Utc::now(),
            false,
        );
        let provider = Provider::new(cache, Some("https://custom.codecov.io"));
        assert_eq!(provider.base_url, "https://custom.codecov.io");
    }
}
