use super::client::{Client, HostingApiResult, Issue, IssueState, RateLimitInfo, Repository};
use crate::Result;
use crate::facts::ProviderResult;
use crate::facts::cache_doc;
use crate::facts::crate_spec::{self, CrateSpec};
use crate::facts::hosting::{AgeStats, HostingData, IssueStats};
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::repo_spec::RepoSpec;
use crate::facts::request_tracker::{RequestTracker, TrackedTopic};
use chrono::{DateTime, Utc};
use core::time::Duration;
use futures_util::future::join_all;
use ohno::{EnrichableExt, IntoAppError};
use reqwest::header::LINK;
use serde::de::IgnoredAny;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

const LOG_TARGET: &str = "   hosting";
const SECONDS_PER_DAY: f64 = 86400.0;
const COMMIT_LOOKBACK_DAYS: i64 = 90;
const ISSUE_LOOKBACK_DAYS: i64 = 365 * 10;
const ISSUE_PAGE_SIZE: u8 = 100;
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 3600;

/// Initial batch size for repository requests
const INITIAL_BATCH_SIZE: usize = 16;

/// Maximum batch size for repository requests
const MAX_BATCH_SIZE: usize = 64;

/// Estimated number of GitHub API requests per repository
/// Each repo requires ~6 requests: repo info, contributors, commits, issues (with potential pagination)
const REQUESTS_PER_REPO: usize = 6;

/// Pattern to extract page number from GitHub API Link header
static PAGE_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r#"page=(\d+)>; rel="last""#).expect("invalid regex"));

/// Macro to unwrap `HostingApiResult` or propagate rate limit/error
macro_rules! unwrap_or_return {
    ($expr:expr) => {
        match $expr {
            HostingApiResult::Success(data, rate_limit) => (data, rate_limit),
            HostingApiResult::RateLimited(rate_limit) => return HostingApiResult::RateLimited(rate_limit),
            HostingApiResult::Failed(e, rate_limit) => return HostingApiResult::Failed(e, rate_limit),
        }
    };
}

/// Macro to unwrap `HostingApiResult` for repo data operations or return early with `RepoData` error
/// Accepts closures for context/warning messages to avoid eager string allocation
/// Warning message is optional - if provided, logs on failure
macro_rules! unwrap_repo_result {
    ($expr:expr, $repo_spec:expr, $context_msg:expr $(, $warn_msg:expr)?) => {
        match $expr {
            HostingApiResult::Success(data, rate_limit) => (data, rate_limit),
            HostingApiResult::RateLimited(rate_limit) => {
                return RepoData {
                    repo_spec: $repo_spec,
                    result: ProviderResult::Error(Arc::new(ohno::app_err!("Rate limited"))),
                    rate_limit: Some(rate_limit),
                    is_rate_limited: true,
                };
            }
            HostingApiResult::Failed(e, rate_limit) => {
                $(
                    let warn_msg_str = ($warn_msg)();
                    log::warn!(target: LOG_TARGET, "{}: {:#}", warn_msg_str, e);
                )?
                let error = Arc::new(e.enrich_with(|| ($context_msg)()));
                return RepoData {
                    repo_spec: $repo_spec,
                    result: ProviderResult::Error(error),
                    rate_limit,
                    is_rate_limited: false,
                };
            }
        }
    };
}

/// Result of fetching hosting data for a repository
#[derive(Debug, Clone)]
struct RepoData {
    repo_spec: RepoSpec,
    result: ProviderResult<HostingData>,
    rate_limit: Option<RateLimitInfo>,
    is_rate_limited: bool,
}

impl RepoData {
    /// Create `RepoData` for a repository that was not found
    const fn not_found(repo_spec: RepoSpec) -> Self {
        Self {
            repo_spec,
            result: ProviderResult::CrateNotFound,
            rate_limit: None,
            is_rate_limited: false,
        }
    }

    /// Create `RepoData` from cached data
    const fn from_cache(repo_spec: RepoSpec, data: HostingData) -> Self {
        Self {
            repo_spec,
            result: ProviderResult::Found(data),
            rate_limit: None,
            is_rate_limited: false,
        }
    }

    /// Create `RepoData` from successful fetch
    const fn success(repo_spec: RepoSpec, result: ProviderResult<HostingData>, rate_limit: Option<RateLimitInfo>) -> Self {
        Self {
            repo_spec,
            result,
            rate_limit,
            is_rate_limited: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Provider {
    client: Client,
    cache_dir: Arc<Path>,
    cache_ttl: Duration,
}

impl Provider {
    pub fn new(token: Option<&str>, cache_dir: impl AsRef<Path>, cache_ttl: Duration) -> Result<Self> {
        Ok(Self {
            client: Client::new(token)?,
            cache_dir: Arc::from(cache_dir.as_ref()),
            cache_ttl,
        })
    }

    pub async fn get_hosting_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
        tracker: &RequestTracker,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<HostingData>)> {
        let mut repo_to_crates = crate_spec::by_repo(crates);

        let mut pending_repos: Vec<RepoSpec> = repo_to_crates.keys().cloned().collect();
        tracker.add_requests(TrackedTopic::GitHub, pending_repos.len() as u64);

        let mut results = Vec::with_capacity(pending_repos.len());
        let mut next_batch_size = INITIAL_BATCH_SIZE;

        while !pending_repos.is_empty() {
            let batch_size = next_batch_size.min(pending_repos.len());
            let batch = pending_repos.split_off(pending_repos.len() - batch_size);

            log::debug!(
                target: LOG_TARGET,
                "Processing batch of {} repos ({} remaining)",
                batch.len(),
                pending_repos.len()
            );

            // Fetch data for all repos in this batch concurrently
            let batch_futures = batch.into_iter().map(|repo_spec| self.fetch_hosting_data_for_repo(repo_spec));
            let batch_results = join_all(batch_futures).await;

            // Separate rate-limited repos from completed ones, and collect rate limit info
            let mut rate_limited_repos = Vec::new();
            let mut latest_rate_limit: Option<RateLimitInfo> = None;
            let mut latest_reset_time: Option<DateTime<Utc>> = None;

            for repo_data in batch_results {
                if repo_data.is_rate_limited {
                    // Rate limit hit - queue for retry and track the latest reset time
                    rate_limited_repos.push(repo_data.repo_spec);
                    if let Some(rate_limit) = repo_data.rate_limit {
                        latest_reset_time =
                            Some(latest_reset_time.map_or(rate_limit.reset_at, |existing| existing.max(rate_limit.reset_at)));
                    }
                } else {
                    // Success or non-rate-limit error - add to results and capture rate limit info
                    if let Some(rate_limit) = repo_data.rate_limit {
                        latest_rate_limit = Some(rate_limit);
                    }
                    results.push(repo_data);
                    tracker.complete_request(TrackedTopic::GitHub);
                }
            }

            // Handle rate limiting
            if rate_limited_repos.is_empty() {
                // No rate limits hit - use rate limit info from responses to adjust batch size
                if let Some(rate_limit) = latest_rate_limit {
                    let remaining = rate_limit.remaining;
                    // Calculate how many repos we can handle with remaining quota
                    let repos_possible = remaining / REQUESTS_PER_REPO;
                    next_batch_size = repos_possible.clamp(1, MAX_BATCH_SIZE);

                    log::debug!(
                        target: LOG_TARGET,
                        "Rate limit status: remaining={remaining}, next_batch_size={next_batch_size}"
                    );
                } else {
                    // No rate limit info available - keep current batch size
                    log::debug!(
                        target: LOG_TARGET,
                        "No rate limit info available, keeping batch size at {next_batch_size}"
                    );
                }
            } else {
                log::warn!(
                    target: LOG_TARGET,
                    "Hit rate limit on {} repos",
                    rate_limited_repos.len()
                );

                // Put rate-limited repos back in the queue
                pending_repos.extend(rate_limited_repos);

                // Use the latest reset time from rate limit info, or fallback to 1 hour
                let now = Utc::now();
                let reset_time = latest_reset_time.unwrap_or_else(|| now + chrono::Duration::hours(1));
                let wait_until = reset_time.min(now + chrono::Duration::seconds(MAX_RATE_LIMIT_WAIT_SECS.cast_signed()));

                if wait_until > now {
                    let formatted_time = wait_until.with_timezone(&chrono::Local).format("%T");
                    eprintln!("GitHub rate limit exceeded: Waiting until {formatted_time}...");

                    let wait_duration = (wait_until - now).to_std().unwrap_or(Duration::ZERO);
                    tokio::time::sleep(wait_duration).await;
                }

                // After waiting, reset to initial batch size
                next_batch_size = INITIAL_BATCH_SIZE;
            }
        }

        // Map results back to crates
        results
            .into_iter()
            .flat_map(move |repo_data| {
                let crate_specs = repo_to_crates.remove(&repo_data.repo_spec).expect("repo_spec must exist");
                crate_specs
                    .into_iter()
                    .map(move |crate_spec| (crate_spec, repo_data.result.clone()))
            })
            .inspect(|(crate_spec, result)| {
                if let ProviderResult::Error(e) = result {
                    log::error!(target: LOG_TARGET, "Could not fetch hosting data for {crate_spec}: {e:#}");
                } else if matches!(result, ProviderResult::CrateNotFound) {
                    log::warn!(target: LOG_TARGET, "Could not find {crate_spec}");
                }
            })
    }

    /// Fetch repository data for a single repository
    async fn fetch_hosting_data_for_repo(&self, repo_spec: RepoSpec) -> RepoData {
        // Only handle GitHub repositories for now
        if repo_spec.host() != "github.com" {
            log::debug!(target: LOG_TARGET, "Skipping non-GitHub repository: {repo_spec}");
            return RepoData::not_found(repo_spec);
        }

        let owner = repo_spec.owner();
        let repo = repo_spec.repo();

        let cache_path = self.get_cache_path(owner, repo);
        if let Some(data) = cache_doc::load_with_ttl(
            &cache_path,
            self.cache_ttl,
            |data: &HostingData| data.timestamp,
            format!("hosting data for repository '{repo_spec}'"),
        ) {
            return RepoData::from_cache(repo_spec, data);
        }

        log::info!(target: LOG_TARGET, "Querying GitHub for hosting information on repository '{repo_spec}'");

        let (repo_res, contributors_res, commits_res, issues_res) = tokio::join!(
            self.get_repo_info(owner, repo),
            self.get_contributors_count(owner, repo),
            self.get_commits_count(owner, repo),
            self.get_issues_and_pulls(owner, repo)
        );

        // Check for rate limiting or permanent failures in each result
        let (repo_data, repo_rate_limit) = unwrap_repo_result!(repo_res, repo_spec, || Self::error_context("core info", &repo_spec));

        let ((issues, pulls), issues_rate_limit) = unwrap_repo_result!(
            issues_res,
            repo_spec,
            || Self::error_context("issues and pull request info", &repo_spec),
            || Self::error_warning("issues/PRs", &repo_spec)
        );

        let (contributors, contributors_rate_limit) = unwrap_repo_result!(
            contributors_res,
            repo_spec,
            || Self::error_context("contributor count", &repo_spec),
            || Self::error_warning("contributors", &repo_spec)
        );

        let (commits, commits_rate_limit) =
            unwrap_repo_result!(commits_res, repo_spec, || Self::error_context("commit count", &repo_spec), || {
                Self::error_warning("commits", &repo_spec)
            });

        // Use the most conservative rate limit info (the one with the least remaining quota)
        let rate_limit = [issues_rate_limit, commits_rate_limit, contributors_rate_limit, repo_rate_limit]
            .into_iter()
            .flatten()
            .min_by_key(|rl| rl.remaining);

        let hosting_data = HostingData {
            timestamp: Utc::now(),
            stars: u64::from(repo_data.stargazers_count.unwrap_or(0)),
            forks: u64::from(repo_data.forks_count.unwrap_or(0)),
            subscribers: repo_data
                .subscribers_count
                .filter(|&count| count >= 0)
                .map_or(0, i64::cast_unsigned),
            contributors,
            commits_last_3_months: commits,
            issues,
            pulls,
        };

        log::debug!(target: LOG_TARGET, "Completed GitHub API requests for repository '{repo_spec}'");

        let result = match cache_doc::save(&hosting_data, &cache_path) {
            Ok(()) => ProviderResult::Found(hosting_data),
            Err(e) => ProviderResult::Error(Arc::new(e)),
        };

        RepoData::success(repo_spec, result, rate_limit)
    }

    /// Generate error context message for failed API calls
    fn error_context(operation: &str, repo_spec: &RepoSpec) -> String {
        format!("could not fetch {operation} for repository '{repo_spec}'")
    }

    /// Generate warning message for failed API calls
    fn error_warning(operation: &str, repo_spec: &RepoSpec) -> String {
        format!("Could not fetch {operation} for '{repo_spec}'")
    }

    /// Get the cache file path for a specific repository
    fn get_cache_path(&self, owner: &str, repo: &str) -> PathBuf {
        let safe_owner = sanitize_path_component(owner);
        let safe_repo = sanitize_path_component(repo);
        self.cache_dir.join(&safe_owner).join(format!("{safe_repo}.json"))
    }

    /// Construct GitHub API URL for a repository with optional path suffix
    fn repo_url(owner: &str, repo: &str, suffix: &str) -> String {
        format!("https://api.github.com/repos/{owner}/{repo}{suffix}")
    }

    async fn get_count_via_link_header(&self, url: &str) -> HostingApiResult<u64> {
        let (resp, rate_limit) = unwrap_or_return!(self.client.api_call(url).await);

        // Try to get count from Link header
        if let Some(link_header) = resp.headers().get(LINK)
            && let Ok(link_str) = link_header.to_str()
            && let Some(count) = PAGE_REGEX.captures(link_str).and_then(|caps| caps.get(1))
        {
            log::debug!(target: LOG_TARGET, "Fetched count via Link header from '{url}'");
            if let Ok(parsed_count) = count.as_str().parse() {
                return HostingApiResult::Success(parsed_count, rate_limit);
            }
        }

        // Download response as bytes and parse
        let bytes = match resp.bytes().await {
            Ok(b) => b,
            Err(e) => return HostingApiResult::Failed(e.into(), rate_limit),
        };

        log::debug!(target: LOG_TARGET, "Fetched response from '{url}'");

        match count_json_array_elements(&bytes).into_app_err_with(|| format!("could not count items in JSON response from '{url}'")) {
            Ok(count) => HostingApiResult::Success(count, rate_limit),
            Err(e) => HostingApiResult::Failed(e, rate_limit),
        }
    }

    async fn get_repo_info(&self, owner: &str, repo: &str) -> HostingApiResult<Repository> {
        let url = Self::repo_url(owner, repo, "");

        let (resp, rate_limit) = unwrap_or_return!(self.client.api_call(&url).await);
        match resp.json().await {
            Ok(repo_info) => HostingApiResult::Success(repo_info, rate_limit),
            Err(e) => HostingApiResult::Failed(e.into(), rate_limit),
        }
    }

    async fn get_issues_and_pulls(&self, owner: &str, repo: &str) -> HostingApiResult<(IssueStats, IssueStats)> {
        let since = Utc::now() - chrono::Duration::days(ISSUE_LOOKBACK_DAYS);
        let since_str = since.to_rfc3339();

        log::debug!(target: LOG_TARGET, "Fetching issues and pull requests for '{owner}/{repo}'");

        let mut all_issues = Vec::new();
        let mut latest_rate_limit: Option<RateLimitInfo> = None;
        let mut page_num = 1u32;

        loop {
            let url = format!(
                "https://api.github.com/repos/{owner}/{repo}/issues?state=all&since={since_str}&per_page={ISSUE_PAGE_SIZE}&page={page_num}"
            );

            let (resp, rate_limit) = unwrap_or_return!(self.client.api_call(&url).await);

            // Update latest rate limit info
            latest_rate_limit = rate_limit.or(latest_rate_limit);

            // Parse next page link if present
            let has_next_page = resp
                .headers()
                .get(LINK)
                .and_then(|h| h.to_str().ok())
                .is_some_and(|link_str| link_str.contains(r#"rel="next""#));

            let issues: Vec<Issue> = match resp.json().await {
                Ok(i) => i,
                Err(e) => return HostingApiResult::Failed(e.into(), latest_rate_limit),
            };

            if issues.is_empty() {
                break;
            }

            all_issues.extend(issues);

            if !has_next_page {
                break;
            }

            page_num += 1;
        }

        log::debug!(target: LOG_TARGET, "Fetched {} issues and pull requests for '{owner}/{repo}'", all_issues.len());

        // Pre-allocate vectors with rough capacity estimate to reduce reallocations
        let total = all_issues.len();
        let mut open_issues = Vec::with_capacity(total / 4);
        let mut closed_issues = Vec::with_capacity(total / 4);
        let mut open_pulls = Vec::with_capacity(total / 4);
        let mut closed_pulls = Vec::with_capacity(total / 4);

        for issue in all_issues {
            let is_pr = issue.pull_request.is_some();
            let is_open = issue.state == IssueState::Open;

            if is_pr {
                if is_open {
                    open_pulls.push(issue);
                } else {
                    closed_pulls.push(issue);
                }
            } else if is_open {
                open_issues.push(issue);
            } else {
                closed_issues.push(issue);
            }
        }

        let issues_stats = compute_issue_stats(&open_issues, &closed_issues);
        let pulls_stats = compute_issue_stats(&open_pulls, &closed_pulls);

        HostingApiResult::Success((issues_stats, pulls_stats), latest_rate_limit)
    }

    async fn get_contributors_count(&self, owner: &str, repo: &str) -> HostingApiResult<u64> {
        let url = Self::repo_url(owner, repo, "/contributors?per_page=1&anon=true");
        self.get_count_via_link_header(&url).await
    }

    async fn get_commits_count(&self, owner: &str, repo: &str) -> HostingApiResult<u64> {
        let since = Utc::now() - chrono::Duration::days(COMMIT_LOOKBACK_DAYS);
        let since_str = since.to_rfc3339();
        let url = format!("https://api.github.com/repos/{owner}/{repo}/commits?since={since_str}&per_page=1");
        self.get_count_via_link_header(&url).await
    }
}

/// Count elements in a JSON array without allocating parsed values.
fn count_json_array_elements(json: &[u8]) -> Result<u64> {
    let array: Vec<IgnoredAny> = serde_json::from_slice(json).into_app_err("malformed JSON while counting array elements")?;
    Ok(array.len() as u64)
}

#[expect(clippy::cast_precision_loss, reason = "it happens")]
#[expect(clippy::cast_possible_truncation, reason = "it happens")]
#[expect(clippy::cast_sign_loss, reason = "it happens")]
fn compute_age(issues: &[Issue], is_open: bool) -> AgeStats {
    let now = Utc::now();
    let mut seconds: Vec<f64> = issues
        .iter()
        .filter_map(|issue| {
            let age_seconds = if is_open {
                (now - issue.created_at).num_seconds() as f64
            } else {
                issue
                    .closed_at
                    .map_or(0.0, |closed_at| (closed_at - issue.created_at).num_seconds() as f64)
            };
            // Filter out NaN and negative values
            (age_seconds.is_finite() && age_seconds >= 0.0).then_some(age_seconds)
        })
        .collect();

    if seconds.is_empty() {
        return AgeStats::default();
    }

    // Now we can safely use unwrap since we filtered out NaN values
    seconds.sort_by(|a, b| a.partial_cmp(b).expect("no NaN values should be present"));

    AgeStats {
        avg: (seconds.iter().sum::<f64>() / seconds.len() as f64 / SECONDS_PER_DAY) as u32,
        p50: (percentile(&seconds, 50.0) / SECONDS_PER_DAY) as u32,
        p75: (percentile(&seconds, 75.0) / SECONDS_PER_DAY) as u32,
        p90: (percentile(&seconds, 90.0) / SECONDS_PER_DAY) as u32,
        p95: (percentile(&seconds, 95.0) / SECONDS_PER_DAY) as u32,
    }
}

fn percentile(sorted_data: &[f64], percentile: f64) -> f64 {
    if sorted_data.is_empty() {
        return 0.0;
    }

    #[expect(clippy::cast_possible_truncation, reason = "index calculation")]
    #[expect(clippy::cast_sign_loss, reason = "index is positive")]
    #[expect(clippy::cast_precision_loss, reason = "index fits in usize")]
    let idx = (percentile / 100.0 * (sorted_data.len() - 1) as f64).round() as usize;
    sorted_data[idx]
}

/// Helper to create `IssueStats` from open and closed issue lists
fn compute_issue_stats(open: &[Issue], closed: &[Issue]) -> IssueStats {
    IssueStats {
        open_count: open.len() as u64,
        closed_count: closed.len() as u64,
        open_age: compute_age(open, true),
        closed_age: compute_age(closed, false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_json_array_elements() {
        // Empty array
        assert_eq!(count_json_array_elements(b"[]").unwrap(), 0);

        // Single element
        assert_eq!(count_json_array_elements(br#"[{"id": 1}]"#).unwrap(), 1);

        // Multiple elements
        assert_eq!(count_json_array_elements(br#"[{"id": 1}, {"id": 2}, {"id": 3}]"#).unwrap(), 3);

        // Complex objects (like GitHub contributors)
        let json = br#"[
            {"login": "user1", "contributions": 100},
            {"login": "user2", "contributions": 50},
            {"login": "user3", "contributions": 25}
        ]"#;
        assert_eq!(count_json_array_elements(json).unwrap(), 3);

        // Malformed JSON should error
        let _ = count_json_array_elements(b"[{broken").unwrap_err();
    }
}
