use super::client::{Client, HostingApiResult, Issue, IssueState, RateLimitInfo, Repository};
use super::{AgeStats, HostingData, IssueStats};
use crate::Result;
use crate::facts::ProviderResult;
use crate::facts::RepoSpec;
use crate::facts::cache_doc;
use crate::facts::crate_spec::{self, CrateSpec};
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::request_tracker::{RequestTracker, TrackedTopic};
use chrono::{DateTime, Utc};
use core::time::Duration;
use futures_util::future::join_all;
use ohno::{EnrichableExt, IntoAppError};
use reqwest::header::LINK;
use serde::de::IgnoredAny;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

const LOG_TARGET: &str = "   hosting";
const SECONDS_PER_DAY: f64 = 86400.0;
const COMMIT_LOOKBACK_DAYS: i64 = 90;
const ISSUE_LOOKBACK_DAYS: i64 = 365 * 10;
const ISSUE_PAGE_SIZE: u8 = 255;
const MAX_ISSUE_PAGES: u32 = 10;
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 3600;

/// Initial batch size for repository requests
const INITIAL_BATCH_SIZE: usize = 16;

/// Maximum batch size for repository requests
const MAX_BATCH_SIZE: usize = 64;

/// Estimated number of API requests per repository
/// Each repo requires approximately 3 requests: repo info, commits, issues
const ESTIMATED_REQUESTS_PER_REPO: usize = 3;

/// Pattern to extract page number from GitHub API Link header
static PAGE_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r#"page=(\d+)>; rel="last""#).expect("invalid regex"));

/// Configuration for a specific hosting provider
#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_field_names, reason = "host_domain is a clear and reasonable field name")]
struct Host {
    /// Host domain (e.g., `github.com`, `Codeberg.org`)
    host_domain: &'static str,
    /// Base API URL
    base_url: &'static str,
    /// Display name for error messages
    display_name: &'static str,
    /// Whether to use `watchers_count` field instead of `subscribers_count`
    use_watchers_for_subscribers: bool,
}

/// Supported hosting providers
static SUPPORTED_HOSTS: &[Host] = &[
    Host {
        host_domain: "github.com",
        base_url: "https://api.github.com",
        display_name: "GitHub",
        use_watchers_for_subscribers: false,
    },
    Host {
        host_domain: "codeberg.org",
        base_url: "https://codeberg.org/api/v1",
        display_name: "Codeberg",
        use_watchers_for_subscribers: true,
    },
];

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
/// Takes operation name strings and constructs error messages
/// Warning is optional - if provided, logs on failure
macro_rules! unwrap_repo_result {
    ($expr:expr, $repo_spec:expr, $operation:expr $(, $warn_operation:expr)?) => {
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
                    log::warn!(target: LOG_TARGET, "Could not fetch {} for '{}': {:#}", $warn_operation, $repo_spec, e);
                )?
                let error = Arc::new(e.enrich_with(|| format!("could not fetch {} for repository '{}'", $operation, $repo_spec)));
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
    hosts: Vec<(Host, Client)>,
    cache_dir: Arc<Path>,
    cache_ttl: Duration,
    now: DateTime<Utc>,
}

impl Provider {
    pub fn new(
        github_token: Option<&str>,
        codeberg_token: Option<&str>,
        cache_dir: impl AsRef<Path>,
        cache_ttl: Duration,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        let mut hosts = Vec::new();

        for host in SUPPORTED_HOSTS {
            // Map host domain to appropriate token
            let token = match host.host_domain {
                "github.com" => github_token,
                "codeberg.org" => codeberg_token,
                _ => None,
            };

            let client = Client::new(token, host.base_url, now)?;
            hosts.push((*host, client));
        }

        Ok(Self {
            hosts,
            cache_dir: Arc::from(cache_dir.as_ref()),
            cache_ttl,
            now,
        })
    }

    pub async fn get_hosting_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
        tracker: &RequestTracker,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<HostingData>)> {
        let repo_to_crates = crate_spec::by_repo(crates);

        // Group repos by host domain
        let mut repos_by_host: HashMap<&'static str, Vec<RepoSpec>> = HashMap::new();
        let mut crates_by_host: HashMap<&'static str, HashMap<RepoSpec, Vec<CrateSpec>>> = HashMap::new();
        let mut unknown_host_crates: Vec<(CrateSpec, String)> = Vec::new();

        for (repo_spec, crate_specs) in repo_to_crates {
            let host_domain = repo_spec.host().to_string();

            // Check if this host is supported
            if let Some(host) = SUPPORTED_HOSTS.iter().find(|h| h.host_domain == host_domain) {
                repos_by_host.entry(host.host_domain).or_default().push(repo_spec.clone());
                let _ = crates_by_host.entry(host.host_domain).or_default().insert(repo_spec, crate_specs);
            } else {
                log::warn!(target: LOG_TARGET, "Unsupported host '{host_domain}', cannot fetch hosting data");
                for crate_spec in crate_specs {
                    unknown_host_crates.push((crate_spec, host_domain.clone()));
                }
            }
        }

        // Track requests for each supported host
        for repos in repos_by_host.values() {
            tracker.add_requests(TrackedTopic::Repos, repos.len() as u64);
        }

        // Process each supported host in parallel
        let mut fetch_futures = Vec::new();
        for (host, client) in &self.hosts {
            if let Some(repos) = repos_by_host.get(host.host_domain) {
                let fut = self.fetch_hosting_data_batch(client, repos.clone(), host, tracker);
                fetch_futures.push(fut);
            }
        }

        let all_results = join_all(fetch_futures).await;

        // Merge all repo-to-crates maps for efficient lookup
        let mut repo_to_crates_all = HashMap::new();
        for crates_map in crates_by_host.into_values() {
            repo_to_crates_all.extend(crates_map);
        }

        // Flatten results from all hosts and map back to crates
        let known_host_results = all_results.into_iter().flatten().flat_map(move |repo_data| {
            let crate_specs = repo_to_crates_all.remove(&repo_data.repo_spec).expect("repo_spec must exist");
            crate_specs
                .into_iter()
                .map(move |crate_spec| (crate_spec, repo_data.result.clone()))
        });

        // Create error results for crates from unknown hosts
        let unknown_host_results = unknown_host_crates.into_iter().map(|(crate_spec, host_domain)| {
            let error = Arc::new(ohno::app_err!("Unsupported hosting provider: {}", host_domain));
            (crate_spec, ProviderResult::Error(error))
        });

        // Chain all results together
        known_host_results.chain(unknown_host_results).inspect(|(crate_spec, result)| {
            if let ProviderResult::Error(e) = result {
                log::error!(target: LOG_TARGET, "Could not fetch hosting data for {crate_spec}: {e:#}");
            } else if matches!(result, ProviderResult::CrateNotFound(_)) {
                log::warn!(target: LOG_TARGET, "Could not find {crate_spec}");
            }
        })
    }

    /// Process a batch of repositories using a specific client
    async fn fetch_hosting_data_batch(
        &self,
        client: &Client,
        mut pending_repos: Vec<RepoSpec>,
        host: &Host,
        tracker: &RequestTracker,
    ) -> Vec<RepoData> {
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
            let batch_futures = batch
                .into_iter()
                .map(|repo_spec| self.fetch_hosting_data_for_repo(client, host, repo_spec));
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
                    tracker.complete_request(TrackedTopic::Repos);
                }
            }

            // Handle rate limiting
            if rate_limited_repos.is_empty() {
                // No rate limits hit - use rate limit info from responses to adjust batch size
                if let Some(rate_limit) = latest_rate_limit {
                    let remaining = rate_limit.remaining;
                    // Calculate how many repos we can handle with remaining quota
                    let repos_possible = remaining / ESTIMATED_REQUESTS_PER_REPO;
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
                let reset_time = latest_reset_time.unwrap_or_else(|| self.now + chrono::Duration::hours(1));
                let wait_until = reset_time.min(self.now + chrono::Duration::seconds(MAX_RATE_LIMIT_WAIT_SECS.cast_signed()));

                if wait_until > self.now {
                    let formatted_time = wait_until.with_timezone(&chrono::Local).format("%T");
                    eprintln!("{} rate limit exceeded: Waiting until {formatted_time}...", host.display_name);

                    let wait_duration = (wait_until - self.now).to_std().unwrap_or(Duration::ZERO);
                    tokio::time::sleep(wait_duration).await;
                }

                // After waiting, reset to initial batch size
                next_batch_size = INITIAL_BATCH_SIZE;
            }
        }

        results
    }

    /// Fetch repository data for a single repository
    async fn fetch_hosting_data_for_repo(&self, client: &Client, host: &Host, repo_spec: RepoSpec) -> RepoData {
        let owner = repo_spec.owner();
        let repo = repo_spec.repo();

        let cache_path = self.get_cache_path(host.host_domain, owner, repo);
        if let Some(data) = cache_doc::load_with_ttl(
            &cache_path,
            self.cache_ttl,
            |data: &HostingData| data.timestamp,
            self.now,
            format!("hosting data for repository '{repo_spec}'"),
        ) {
            return RepoData::from_cache(repo_spec, data);
        }

        log::info!(target: LOG_TARGET, "Querying hosting API for information on repository '{repo_spec}'");

        let (repo_res, commits_res, issues_res) = tokio::join!(
            self.get_repo_info(client, owner, repo),
            self.get_commits_count(client, owner, repo),
            self.get_issues_and_pulls(client, owner, repo)
        );

        // Check for rate limiting or permanent failures in each result
        let (repo_data, repo_rate_limit) = unwrap_repo_result!(repo_res, repo_spec, "core info");
        let ((issues, pulls), issues_rate_limit) = unwrap_repo_result!(issues_res, repo_spec, "issues and pull request info", "issues/PRs");
        let (commits, commits_rate_limit) = unwrap_repo_result!(commits_res, repo_spec, "commit count", "commits");

        // Use the most conservative rate limit info (the one with the least remaining quota)
        let rate_limit = [issues_rate_limit, commits_rate_limit, repo_rate_limit]
            .into_iter()
            .flatten()
            .min_by_key(|rl| rl.remaining);

        // GitHub uses subscribers_count, Codeberg uses watchers_count
        let subscribers = if host.use_watchers_for_subscribers {
            repo_data.watchers_count
        } else {
            repo_data.subscribers_count
        }
        .filter(|&count| count >= 0)
        .map_or(0, i64::cast_unsigned);

        let hosting_data = HostingData {
            timestamp: self.now,
            stars: u64::from(repo_data.stargazers_count.unwrap_or(0)),
            forks: u64::from(repo_data.forks_count.unwrap_or(0)),
            subscribers,
            commits_last_90_days: commits,
            issues,
            pulls,
        };

        log::debug!(target: LOG_TARGET, "Completed hosting API requests for repository '{repo_spec}'");

        let result = match cache_doc::save(&hosting_data, &cache_path) {
            Ok(()) => ProviderResult::Found(hosting_data),
            Err(e) => ProviderResult::Error(Arc::new(e)),
        };

        RepoData::success(repo_spec, result, rate_limit)
    }

    /// Get the cache file path for a specific repository
    fn get_cache_path(&self, host_domain: &str, owner: &str, repo: &str) -> PathBuf {
        let safe_host = sanitize_path_component(host_domain);
        let safe_owner = sanitize_path_component(owner);
        let safe_repo = sanitize_path_component(repo);
        self.cache_dir.join(&safe_host).join(&safe_owner).join(format!("{safe_repo}.json"))
    }

    /// Construct API URL for a repository with optional path suffix
    fn repo_url(client: &Client, owner: &str, repo: &str, suffix: &str) -> String {
        format!("{}/repos/{owner}/{repo}{suffix}", client.base_url())
    }

    async fn get_count_via_link_header(&self, client: &Client, url: &str) -> HostingApiResult<u64> {
        let (resp, rate_limit) = unwrap_or_return!(client.api_call(url).await);

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

    async fn get_repo_info(&self, client: &Client, owner: &str, repo: &str) -> HostingApiResult<Repository> {
        let url = Self::repo_url(client, owner, repo, "");

        let (resp, rate_limit) = unwrap_or_return!(client.api_call(&url).await);
        match resp.json().await {
            Ok(repo_info) => HostingApiResult::Success(repo_info, rate_limit),
            Err(e) => HostingApiResult::Failed(e.into(), rate_limit),
        }
    }

    async fn get_issues_and_pulls(&self, client: &Client, owner: &str, repo: &str) -> HostingApiResult<(IssueStats, IssueStats)> {
        let since = self.now - chrono::Duration::days(ISSUE_LOOKBACK_DAYS);
        let since_str = since.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        let mut all_issues = Vec::new();
        let mut latest_rate_limit: Option<RateLimitInfo> = None;
        let mut page_num = 1u32;

        loop {
            let url = format!(
                "{}/repos/{owner}/{repo}/issues?state=all&since={since_str}&per_page={ISSUE_PAGE_SIZE}&page={page_num}",
                client.base_url()
            );

            let (resp, rate_limit) = unwrap_or_return!(client.api_call(&url).await);

            // Update rate limit info - keep the most conservative (lowest remaining)
            latest_rate_limit = [latest_rate_limit, rate_limit].into_iter().flatten().min_by_key(|rl| rl.remaining);

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

            if page_num > MAX_ISSUE_PAGES {
                log::debug!(target: LOG_TARGET, "Reached maximum issue page limit ({MAX_ISSUE_PAGES}) for '{owner}/{repo}', stopping pagination");
                break;
            }
        }

        let mut open_issues = Vec::new();
        let mut closed_issues = Vec::new();
        let mut open_pulls = Vec::new();
        let mut closed_pulls = Vec::new();

        for issue in all_issues {
            let is_pr = issue.pull_request.is_some();
            let is_open = issue.state == IssueState::Open;

            let target = match (is_pr, is_open) {
                (true, true) => &mut open_pulls,
                (true, false) => &mut closed_pulls,
                (false, true) => &mut open_issues,
                (false, false) => &mut closed_issues,
            };
            target.push(issue);
        }

        let issues_stats = compute_issue_stats(&open_issues, &closed_issues, self.now);
        let pulls_stats = compute_issue_stats(&open_pulls, &closed_pulls, self.now);

        HostingApiResult::Success((issues_stats, pulls_stats), latest_rate_limit)
    }

    async fn get_commits_count(&self, client: &Client, owner: &str, repo: &str) -> HostingApiResult<u64> {
        let since = self.now - chrono::Duration::days(COMMIT_LOOKBACK_DAYS);
        let since_str = since.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let url = format!("{}/repos/{owner}/{repo}/commits?since={since_str}&per_page=1", client.base_url());
        self.get_count_via_link_header(client, &url).await
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
fn compute_age(issues: &[Issue], is_open: bool, now: DateTime<Utc>) -> AgeStats {
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
fn compute_issue_stats(open: &[Issue], closed: &[Issue], now: DateTime<Utc>) -> IssueStats {
    IssueStats {
        open_count: open.len() as u64,
        closed_count: closed.len() as u64,
        open_age: compute_age(open, true, now),
        closed_age: compute_age(closed, false, now),
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

    #[test]
    fn test_percentile_empty() {
        assert!(percentile(&[], 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_percentile_single_element() {
        assert!((percentile(&[42.0], 50.0) - 42.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_percentile_median() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((percentile(&data, 50.0) - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_percentile_75th() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        assert!((percentile(&data, 75.0) - 4.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_percentile_95th() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0];
        assert!((percentile(&data, 95.0) - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_compute_age_empty() {
        let issues: Vec<Issue> = vec![];
        let stats = compute_age(&issues, true, Utc::now());
        assert_eq!(stats.avg, 0);
        assert_eq!(stats.p50, 0);
        assert_eq!(stats.p75, 0);
        assert_eq!(stats.p90, 0);
        assert_eq!(stats.p95, 0);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_compute_age_open_issues() {
        let now = Utc::now();
        let issues = vec![
            Issue {
                created_at: now - chrono::Duration::days(10),
                closed_at: None,
                state: IssueState::Open,
                pull_request: None,
            },
            Issue {
                created_at: now - chrono::Duration::days(20),
                closed_at: None,
                state: IssueState::Open,
                pull_request: None,
            },
            Issue {
                created_at: now - chrono::Duration::days(5),
                closed_at: None,
                state: IssueState::Open,
                pull_request: None,
            },
        ];

        let stats = compute_age(&issues, true, now);
        // Average should be around 11-12 days
        assert!(stats.avg >= 11 && stats.avg <= 12);
        assert!(stats.p50 >= 9 && stats.p50 <= 11);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_compute_age_closed_issues() {
        let now = Utc::now();
        let issues = vec![
            Issue {
                created_at: now - chrono::Duration::days(30),
                closed_at: Some(now - chrono::Duration::days(20)),
                state: IssueState::Closed,
                pull_request: None,
            },
            Issue {
                created_at: now - chrono::Duration::days(25),
                closed_at: Some(now - chrono::Duration::days(20)),
                state: IssueState::Closed,
                pull_request: None,
            },
        ];

        let stats = compute_age(&issues, false, now);
        // Time from creation to close: first was open for 10 days, second for 5 days
        assert!(stats.avg >= 7 && stats.avg <= 8); // Average around 7.5 days
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_compute_issue_stats() {
        let now = Utc::now();
        let open = vec![
            Issue {
                created_at: now - chrono::Duration::days(10),
                closed_at: None,
                state: IssueState::Open,
                pull_request: None,
            },
            Issue {
                created_at: now - chrono::Duration::days(5),
                closed_at: None,
                state: IssueState::Open,
                pull_request: None,
            },
        ];

        let closed = vec![Issue {
            created_at: now - chrono::Duration::days(30),
            closed_at: Some(now - chrono::Duration::days(25)),
            state: IssueState::Closed,
            pull_request: None,
        }];

        let stats = compute_issue_stats(&open, &closed, now);

        assert_eq!(stats.open_count, 2);
        assert_eq!(stats.closed_count, 1);
        assert!(stats.open_age.avg >= 7 && stats.open_age.avg <= 8);
        assert!(stats.closed_age.avg >= 4 && stats.closed_age.avg <= 6);
    }

    #[test]
    fn test_get_cache_path() {
        let now = Utc::now();
        let provider = Provider::new(None, None, "test_cache", Duration::from_secs(3600), now).unwrap();

        let path = provider.get_cache_path("github.com", "tokio-rs", "tokio");

        assert!(path.to_string_lossy().contains("github.com"));
        assert!(path.to_string_lossy().contains("tokio-rs"));
        assert!(path.to_string_lossy().contains("tokio.json"));
    }

    #[test]
    fn test_get_cache_path_sanitized() {
        let now = Utc::now();
        let provider = Provider::new(None, None, "test_cache", Duration::from_secs(3600), now).unwrap();

        let path = provider.get_cache_path("evil.com", "../../../etc", "passwd");

        // Path traversal should be sanitized
        let path_str = path.to_string_lossy();
        assert!(!path_str.contains("../"));
        assert!(path_str.contains("passwd.json"));
    }

    #[test]
    fn test_repo_url() {
        let client = Client::new(None, "https://api.github.com", Utc::now()).unwrap();

        let url = Provider::repo_url(&client, "tokio-rs", "tokio", "");
        assert_eq!(url, "https://api.github.com/repos/tokio-rs/tokio");

        let url_with_suffix = Provider::repo_url(&client, "tokio-rs", "tokio", "/commits");
        assert_eq!(url_with_suffix, "https://api.github.com/repos/tokio-rs/tokio/commits");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_repo_data_from_cache() {
        let repo_spec = RepoSpec::parse(url::Url::parse("https://github.com/tokio-rs/tokio").unwrap()).unwrap();
        let hosting_data = HostingData {
            timestamp: Utc::now(),
            stars: 1000,
            forks: 200,
            subscribers: 50,
            commits_last_90_days: 150,
            issues: IssueStats {
                open_count: 10,
                closed_count: 100,
                open_age: AgeStats::default(),
                closed_age: AgeStats::default(),
            },
            pulls: IssueStats {
                open_count: 5,
                closed_count: 50,
                open_age: AgeStats::default(),
                closed_age: AgeStats::default(),
            },
        };

        let repo_data = RepoData::from_cache(repo_spec.clone(), hosting_data);

        assert_eq!(repo_data.repo_spec, repo_spec);
        assert!(matches!(repo_data.result, ProviderResult::Found(_)));
        assert!(!repo_data.is_rate_limited);
        assert!(repo_data.rate_limit.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_repo_data_success() {
        let repo_spec = RepoSpec::parse(url::Url::parse("https://github.com/tokio-rs/tokio").unwrap()).unwrap();
        let hosting_data = HostingData {
            timestamp: Utc::now(),
            stars: 1000,
            forks: 200,
            subscribers: 50,
            commits_last_90_days: 150,
            issues: IssueStats {
                open_count: 10,
                closed_count: 100,
                open_age: AgeStats::default(),
                closed_age: AgeStats::default(),
            },
            pulls: IssueStats {
                open_count: 5,
                closed_count: 50,
                open_age: AgeStats::default(),
                closed_age: AgeStats::default(),
            },
        };

        let rate_limit = Some(RateLimitInfo {
            remaining: 5000,
            reset_at: DateTime::from_timestamp(1_234_567_890, 0).unwrap(),
        });

        let repo_data = RepoData::success(repo_spec.clone(), ProviderResult::Found(hosting_data), rate_limit);

        assert_eq!(repo_data.repo_spec, repo_spec);
        assert!(!repo_data.is_rate_limited);
        assert!(repo_data.rate_limit.is_some());
        assert_eq!(repo_data.rate_limit.unwrap().remaining, 5000);
    }

    #[test]
    fn test_provider_new() {
        let now = Utc::now();
        let provider = Provider::new(None, None, "test_cache", Duration::from_secs(3600), now).unwrap();
        assert_eq!(provider.hosts.len(), 2); // GitHub and Codeberg
    }

    #[test]
    fn test_provider_new_with_tokens() {
        let now = Utc::now();
        let provider = Provider::new(
            Some("github_token"),
            Some("codeberg_token"),
            "test_cache",
            Duration::from_secs(3600),
            now,
        )
        .unwrap();
        assert_eq!(provider.hosts.len(), 2);
    }
}
