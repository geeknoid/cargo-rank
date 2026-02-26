use super::client::{Client, HostingApiResult, Issue, IssueState, RateLimitInfo, Repository};
use super::{AgeStats, HostingData, TimeWindowStats};
use crate::Result;
use crate::facts::ProviderResult;
use crate::facts::RepoSpec;
use crate::facts::cache::{Cache, CacheResult};
use crate::facts::crate_spec::{self, CrateSpec};
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::request_tracker::{RequestTracker, TopicStatus, TrackedTopic};
use crate::facts::throttler::Throttler;
use chrono::{DateTime, Utc};
use compact_str::CompactString;
use core::time::Duration;
use futures_util::future::join_all;
use ohno::EnrichableExt;
use reqwest::header::LINK;
use crate::HashMap;
use std::sync::Arc;

const LOG_TARGET: &str = "   hosting";
const SECONDS_PER_DAY: f64 = 86400.0;
const ISSUE_LOOKBACK_DAYS: i64 = 365 * 10;
const ISSUE_PAGE_SIZE: u8 = 100;
const MAX_ISSUE_PAGES: u32 = 10;
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 3600;
const MAX_CONCURRENT_REQUESTS: usize = 5;

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
            HostingApiResult::NotFound(rate_limit) => return HostingApiResult::NotFound(rate_limit),
            HostingApiResult::Failed(e, rate_limit) => return HostingApiResult::Failed(e, rate_limit),
        }
    };
}

/// Macro to unwrap `HostingApiResult` for repo data operations or return early with `RepoData` error
/// Takes operation name strings and constructs error messages
/// Warning is optional - if provided, logs on failure
macro_rules! unwrap_repo_result {
    ($expr:expr, $repo_spec:expr, $operation:expr, $cache:expr, $cache_filename:expr $(, $warn_operation:expr)?) => {
        match $expr {
            HostingApiResult::Success(data, rate_limit) => (data, rate_limit),
            HostingApiResult::RateLimited(rate_limit) => {
                return RepoData {
                    repo_spec: $repo_spec,
                    result: ProviderResult::Error(Arc::new(ohno::app_err!("rate limited"))),
                    rate_limit: Some(rate_limit),
                    is_rate_limited: true,
                };
            }
            HostingApiResult::NotFound(rate_limit) => {
                let reason = format!("repository '{}' not found", $repo_spec);
                if let Err(e) = $cache.save_no_data($cache_filename, &reason) {
                    log::debug!(target: LOG_TARGET, "Could not save cache for '{}': {e:#}", $repo_spec);
                }
                return RepoData {
                    repo_spec: $repo_spec,
                    result: ProviderResult::Unavailable(reason.into()),
                    rate_limit,
                    is_rate_limited: false,
                };
            }
            HostingApiResult::Failed(e, rate_limit) => {
                $(
                    log::warn!(target: LOG_TARGET, "Could not fetch {} for '{}': {:#}", $warn_operation, $repo_spec, e);
                )?
                let error = Arc::new(e.enrich_with(|| format!("fetching {} for repository '{}'", $operation, $repo_spec)));
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
    const fn from_cache(repo_spec: RepoSpec, result: ProviderResult<HostingData>) -> Self {
        Self {
            repo_spec,
            result,
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
    cache: Cache,
    throttler: Arc<Throttler>,
}

impl Provider {
    pub fn new(
        github_token: Option<&str>,
        codeberg_token: Option<&str>,
        cache: Cache,
    ) -> Result<Self> {
        let mut hosts = Vec::with_capacity(SUPPORTED_HOSTS.len());

        for host in SUPPORTED_HOSTS {
            // Map host domain to appropriate token
            let token = match host.host_domain {
                "github.com" => github_token,
                "codeberg.org" => codeberg_token,
                _ => None,
            };

            let client = Client::new(token, host.base_url)?;
            hosts.push((*host, client));
        }

        Ok(Self {
            hosts,
            cache,
            throttler: Throttler::new(MAX_CONCURRENT_REQUESTS),
        })
    }

    pub async fn get_hosting_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
        tracker: &RequestTracker,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<HostingData>)> {
        let repo_to_crates = crate_spec::by_repo(crates);

        // Group repos by host domain
        let mut repos_by_host: HashMap<&'static str, Vec<RepoSpec>> = crate::hash_map_with_capacity(SUPPORTED_HOSTS.len());
        let mut crates_by_host: HashMap<&'static str, HashMap<RepoSpec, Vec<CrateSpec>>> = crate::hash_map_with_capacity(SUPPORTED_HOSTS.len());
        let mut unknown_host_crates: Vec<(CrateSpec, CompactString)> = Vec::new();

        for (repo_spec, crate_specs) in repo_to_crates {
            let host_domain = repo_spec.host();

            // Check if this host is supported
            if let Some(host) = SUPPORTED_HOSTS.iter().find(|h| h.host_domain == host_domain) {
                repos_by_host.entry(host.host_domain).or_default().push(repo_spec.clone());
                let _ = crates_by_host.entry(host.host_domain).or_default().insert(repo_spec, crate_specs);
            } else {
                let filename = Self::get_cache_filename(host_domain, repo_spec.owner(), repo_spec.repo());
                let reason: CompactString = format!("unsupported hosting provider: {host_domain}").into();

                match self.cache.load::<HostingData>(&filename) {
                    CacheResult::Miss => {
                        log::debug!(target: LOG_TARGET, "Unsupported host '{host_domain}', cannot fetch hosting data for {repo_spec}");
                        let _ = self.cache.save_no_data(&filename, reason.as_str());
                    }
                    _ => {
                        log::debug!(target: LOG_TARGET, "Using cached unsupported-host result for '{repo_spec}'");
                    }
                }

                for crate_spec in crate_specs {
                    unknown_host_crates.push((crate_spec, reason.clone()));
                }
            }
        }

        // Track requests for each supported host
        for repos in repos_by_host.values() {
            tracker.add_requests(TrackedTopic::Repos, repos.len() as u64);
        }

        // Process each supported host in parallel
        // Dispatch all repos across all hosts through the throttler
        let mut fetch_futures = Vec::new();
        for (host, client) in &self.hosts {
            if let Some(repos) = repos_by_host.remove(host.host_domain) {
                for repo_spec in repos {
                    fetch_futures.push(self.fetch_with_retry(client, host, repo_spec, tracker));
                }
            }
        }

        let all_results = join_all(fetch_futures).await;

        // Merge all repo-to-crates maps for efficient lookup
        let mut repo_to_crates_all = HashMap::default();
        for crates_map in crates_by_host.into_values() {
            repo_to_crates_all.extend(crates_map);
        }

        // Flatten results and map back to crates
        let known_host_results = all_results.into_iter().flat_map(move |repo_data| {
            let crate_specs = repo_to_crates_all.remove(&repo_data.repo_spec).expect("repo_spec must exist");
            crate_specs
                .into_iter()
                .map(move |crate_spec| (crate_spec, repo_data.result.clone()))
        });

        // Create error results for crates from unknown hosts
        let unknown_host_results = unknown_host_crates.into_iter().map(|(crate_spec, reason)| {
            (crate_spec, ProviderResult::Unavailable(reason))
        });

        // Chain all results together
        known_host_results.chain(unknown_host_results).inspect(|(crate_spec, result)| {
            if let ProviderResult::Error(e) = result {
                log::error!(target: LOG_TARGET, "Could not fetch hosting data for {crate_spec}: {e:#}");
            } else if let ProviderResult::Unavailable(reason) = result {
                log::warn!(target: LOG_TARGET, "Hosting data unavailable for {crate_spec}: {reason}");
            }
        })
    }

    /// Fetch hosting data for a repo, retrying on rate limits.
    ///
    /// Acquires a throttler permit before each attempt. On rate limit, pauses
    /// the throttler for all concurrent tasks and retries after the pause.
    async fn fetch_with_retry(
        &self,
        client: &Client,
        host: &Host,
        repo_spec: RepoSpec,
        tracker: &RequestTracker,
    ) -> RepoData {
        loop {
            let _permit = self.throttler.acquire().await;
            let result = self.fetch_hosting_data_for_repo(client, host, repo_spec.clone()).await;

            if result.is_rate_limited {
                if let Some(rl) = &result.rate_limit {
                    log::debug!(
                        target: LOG_TARGET,
                        "{} API rate limit for '{repo_spec}': {} remaining, resets at {}",
                        host.display_name,
                        rl.remaining,
                        rl.reset_at.with_timezone(&chrono::Local).format("%T")
                    );
                }
                if let Some(rate_limit) = result.rate_limit {
                    let now = Utc::now();
                    let reset_time = rate_limit.reset_at;
                    let wait_until = reset_time.min(now + chrono::Duration::seconds(MAX_RATE_LIMIT_WAIT_SECS.cast_signed()));

                    if wait_until > now {
                        let wait_duration = (wait_until - now).to_std().unwrap_or(Duration::ZERO);
                        if self.throttler.pause_for(wait_duration) {
                            tracker.set_topic_status(TrackedTopic::Repos, TopicStatus::Blocked);
                            let formatted_time = wait_until.with_timezone(&chrono::Local).format("%T").to_string();
                            log::warn!(target: LOG_TARGET, "Hit {} rate limit for repository '{repo_spec}'", host.display_name);
                            if !log::log_enabled!(log::Level::Warn) {
                                tracker.println(&format!(
                                    "{} rate limit exceeded: Waiting until {formatted_time}...",
                                    host.display_name
                                ));
                            }

                            let throttler = Arc::clone(&self.throttler);
                            let tracker = tracker.clone();
                            let display_name = host.display_name;
                            drop(tokio::spawn(async move {
                                loop {
                                    tokio::time::sleep(Duration::from_secs(60)).await;
                                    if !throttler.is_paused() {
                                        tracker.set_topic_status(TrackedTopic::Repos, TopicStatus::Active);
                                        log::info!(target: LOG_TARGET, "{display_name} rate limit lifted, resuming requests");
                                        if !log::log_enabled!(log::Level::Info) {
                                            tracker.println(&format!("{display_name} rate limit lifted, resuming requests"));
                                        }
                                        break;
                                    }
                                    let remaining = wait_until - Utc::now();
                                    let remaining_mins = remaining.num_minutes();
                                    if remaining_mins > 0 {
                                        log::info!(
                                            target: LOG_TARGET,
                                            "{display_name} rate limit: ~{remaining_mins} minute(s) remaining until {formatted_time}"
                                        );
                                        if !log::log_enabled!(log::Level::Info) {
                                            tracker.println(&format!(
                                                "{display_name} rate limit: ~{remaining_mins} minute(s) remaining until {formatted_time}"
                                            ));
                                        }
                                    }
                                }
                            }));
                        }
                    }
                }
                continue;
            }

            tracker.complete_request(TrackedTopic::Repos);
            return result;
        }
    }

    /// Fetch repository data for a single repository
    async fn fetch_hosting_data_for_repo(&self, client: &Client, host: &Host, repo_spec: RepoSpec) -> RepoData {
        let owner = repo_spec.owner();
        let repo = repo_spec.repo();

        let filename = Self::get_cache_filename(host.host_domain, owner, repo);
        match self.cache.load::<HostingData>(&filename) {
            CacheResult::Data(data) => return RepoData::from_cache(repo_spec, ProviderResult::Found(data)),
            CacheResult::NoData(reason) => return RepoData::from_cache(repo_spec, ProviderResult::Unavailable(reason.into())),
            CacheResult::Miss => {}
        }

        // If the throttler is paused due to a rate limit detected by another task,
        // skip HTTP calls and signal rate-limited so the caller retries after the pause.
        if self.throttler.is_paused() {
            return RepoData {
                repo_spec,
                result: ProviderResult::Error(Arc::new(ohno::app_err!("rate limited"))),
                rate_limit: None,
                is_rate_limited: true,
            };
        }

        log::info!(target: LOG_TARGET, "Querying {} for information on repository '{repo_spec}'", host.display_name);

        // Run requests sequentially so each throttler permit produces at most one
        // concurrent HTTP request, keeping actual in-flight calls within
        // MAX_CONCURRENT_REQUESTS.
        let repo_res = self.get_repo_info(client, owner, repo).await;

        // Check for rate limiting or permanent failures in each result
        let (repo_data, repo_rate_limit) = unwrap_repo_result!(repo_res, repo_spec, "core info", self.cache, &filename);

        // Bail if another task paused the throttler while we were fetching repo info.
        // Use rate_limit: None so fetch_with_retry doesn't extend the pause with
        // primary rate limit info from the successful repo request.
        if self.throttler.is_paused() {
            return RepoData {
                repo_spec,
                result: ProviderResult::Error(Arc::new(ohno::app_err!("rate limited"))),
                rate_limit: None,
                is_rate_limited: true,
            };
        }

        let issues_res = self.get_issues_and_pulls(client, owner, repo).await;
        let (issue_pull_stats, issues_rate_limit) = unwrap_repo_result!(issues_res, repo_spec, "issues and pull request info", self.cache, &filename, "issues/PRs");

        // Use the most conservative rate limit info (the one with the least remaining quota)
        let rate_limit = [issues_rate_limit, repo_rate_limit]
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
            stars: u64::from(repo_data.stargazers_count.unwrap_or(0)),
            forks: u64::from(repo_data.forks_count.unwrap_or(0)),
            subscribers,
            open_issues: issue_pull_stats.open_issues,
            open_prs: issue_pull_stats.open_prs,
            issues_opened: issue_pull_stats.issues_opened,
            issues_closed: issue_pull_stats.issues_closed,
            prs_opened: issue_pull_stats.prs_opened,
            prs_merged: issue_pull_stats.prs_merged,
            prs_closed: issue_pull_stats.prs_closed,
            open_issue_age: issue_pull_stats.open_issue_age,
            open_pr_age: issue_pull_stats.open_pr_age,
            closed_issue_age: issue_pull_stats.closed_issue_age,
            closed_issue_age_last_90_days: issue_pull_stats.closed_issue_age_last_90_days,
            closed_issue_age_last_180_days: issue_pull_stats.closed_issue_age_last_180_days,
            closed_issue_age_last_365_days: issue_pull_stats.closed_issue_age_last_365_days,
            merged_pr_age: issue_pull_stats.merged_pr_age,
            merged_pr_age_last_90_days: issue_pull_stats.merged_pr_age_last_90_days,
            merged_pr_age_last_180_days: issue_pull_stats.merged_pr_age_last_180_days,
            merged_pr_age_last_365_days: issue_pull_stats.merged_pr_age_last_365_days,
        };

        let total_requests = 1 + issue_pull_stats.request_count;
        log::debug!(target: LOG_TARGET, "Completed {total_requests} {} API request(s) for repository '{repo_spec}'", host.display_name);

        let result = match self.cache.save(&filename, &hosting_data) {
            Ok(()) => ProviderResult::Found(hosting_data),
            Err(e) => ProviderResult::Error(Arc::new(e)),
        };

        RepoData::success(repo_spec, result, rate_limit)
    }

    /// Get the cache filename for a specific repository
    fn get_cache_filename(host_domain: &str, owner: &str, repo: &str) -> String {
        let safe_host = sanitize_path_component(host_domain);
        let safe_owner = sanitize_path_component(owner);
        let safe_repo = sanitize_path_component(repo);
        format!("{safe_host}/{safe_owner}/{safe_repo}.json")
    }

    /// Construct API URL for a repository with optional path suffix
    fn repo_url(client: &Client, owner: &str, repo: &str, suffix: &str) -> String {
        format!("{}/repos/{owner}/{repo}{suffix}", client.base_url())
    }

    async fn get_repo_info(&self, client: &Client, owner: &str, repo: &str) -> HostingApiResult<Repository> {
        let url = Self::repo_url(client, owner, repo, "");

        let (resp, rate_limit) = unwrap_or_return!(client.api_call(&url).await);
        match resp.json().await {
            Ok(repo_info) => HostingApiResult::Success(repo_info, rate_limit),
            Err(e) => HostingApiResult::Failed(e.into(), rate_limit),
        }
    }

    async fn get_issues_and_pulls(&self, client: &Client, owner: &str, repo: &str) -> HostingApiResult<IssueAndPullStats> {
        let since = Utc::now() - chrono::Duration::days(ISSUE_LOOKBACK_DAYS);
        let since_str = since.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);

        let mut all_issues = Vec::with_capacity(ISSUE_PAGE_SIZE as usize);
        let mut latest_rate_limit: Option<RateLimitInfo> = None;
        let mut page_num = 1u32;
        let mut request_count = 0u32;

        loop {
            request_count += 1;
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

            // Stop paginating if another task detected a rate limit.
            // Use a minimal RateLimitInfo with reset_at=now so fetch_with_retry
            // doesn't extend the existing (short) pause with primary rate limit info
            // from successful pagination pages.
            if self.throttler.is_paused() {
                return HostingApiResult::RateLimited(RateLimitInfo {
                    remaining: 0,
                    reset_at: Utc::now(),
                });
            }

            page_num += 1;

            if page_num > MAX_ISSUE_PAGES {
                log::debug!(target: LOG_TARGET, "Reached maximum issue page limit ({MAX_ISSUE_PAGES}) for '{owner}/{repo}', stopping pagination after {} issues", all_issues.len());
                break;
            }
        }

        let mut stats = compute_all_stats(&all_issues, Utc::now());
        stats.request_count = request_count;

        HostingApiResult::Success(stats, latest_rate_limit)
    }
}

/// Compute age statistics from an iterator of durations in seconds.
#[expect(clippy::cast_precision_loss, reason = "acceptable for statistics")]
#[expect(clippy::cast_possible_truncation, reason = "acceptable for day conversion")]
#[expect(clippy::cast_sign_loss, reason = "values are filtered to be non-negative")]
fn compute_age_stats(seconds_iter: impl Iterator<Item = f64>) -> AgeStats {
    let mut seconds: Vec<f64> = seconds_iter
        .filter(|&s| s.is_finite() && s >= 0.0)
        .collect();

    if seconds.is_empty() {
        return AgeStats::default();
    }

    seconds.sort_by(|a, b| a.partial_cmp(b).expect("no NaN values should be present"));

    AgeStats {
        avg: (seconds.iter().sum::<f64>() / seconds.len() as f64 / SECONDS_PER_DAY) as u32,
        p50: (percentile(&seconds, 50.0) / SECONDS_PER_DAY) as u32,
        p75: (percentile(&seconds, 75.0) / SECONDS_PER_DAY) as u32,
        p90: (percentile(&seconds, 90.0) / SECONDS_PER_DAY) as u32,
        p95: (percentile(&seconds, 95.0) / SECONDS_PER_DAY) as u32,
    }
}

/// Duration in seconds from creation to close. Returns `None` if `closed_at` is missing.
#[expect(clippy::cast_precision_loss, reason = "acceptable for duration")]
fn closed_age_seconds(issue: &Issue) -> Option<f64> {
    issue.closed_at.map(|closed_at| (closed_at - issue.created_at).num_seconds() as f64)
}

/// Duration in seconds from creation to merge. Returns `None` if not merged.
#[expect(clippy::cast_precision_loss, reason = "acceptable for duration")]
fn merged_pr_age_seconds(issue: &Issue) -> Option<f64> {
    let merged_at = issue.pull_request.as_ref()?.merged_at?;
    Some((merged_at - issue.created_at).num_seconds() as f64)
}

fn percentile(sorted_data: &[f64], percentile: f64) -> f64 {
    if sorted_data.is_empty() {
        return 0.0;
    }

    #[expect(clippy::cast_possible_truncation, reason = "index calculation")]
    #[expect(clippy::cast_sign_loss, reason = "value is clamped to non-negative range")]
    #[expect(clippy::cast_precision_loss, reason = "index fits in usize")]
    let idx = (percentile / 100.0 * (sorted_data.len() - 1) as f64)
        .round()
        .clamp(0.0, (sorted_data.len() - 1) as f64) as usize;
    sorted_data[idx]
}

/// All computed statistics from the issues/pulls API data.
struct IssueAndPullStats {
    request_count: u32,
    open_issues: u64,
    open_prs: u64,
    issues_opened: TimeWindowStats,
    issues_closed: TimeWindowStats,
    prs_opened: TimeWindowStats,
    prs_merged: TimeWindowStats,
    prs_closed: TimeWindowStats,
    open_issue_age: AgeStats,
    open_pr_age: AgeStats,
    closed_issue_age: AgeStats,
    closed_issue_age_last_90_days: AgeStats,
    closed_issue_age_last_180_days: AgeStats,
    closed_issue_age_last_365_days: AgeStats,
    merged_pr_age: AgeStats,
    merged_pr_age_last_90_days: AgeStats,
    merged_pr_age_last_180_days: AgeStats,
    merged_pr_age_last_365_days: AgeStats,
}

/// Increment time window counters for a given timestamp.
fn increment_window(stats: &mut TimeWindowStats, ts: DateTime<Utc>, cutoff_90: DateTime<Utc>, cutoff_180: DateTime<Utc>, cutoff_365: DateTime<Utc>) {
    stats.total += 1;
    if ts >= cutoff_365 {
        stats.last_365_days += 1;
        if ts >= cutoff_180 {
            stats.last_180_days += 1;
            if ts >= cutoff_90 {
                stats.last_90_days += 1;
            }
        }
    }
}

/// Compute all issue and PR statistics from the raw issue list.
#[expect(clippy::cast_precision_loss, reason = "acceptable for duration")]
fn compute_all_stats(all_issues: &[Issue], now: DateTime<Utc>) -> IssueAndPullStats {
    let cutoff_90 = now - chrono::Duration::days(90);
    let cutoff_180 = now - chrono::Duration::days(180);
    let cutoff_365 = now - chrono::Duration::days(365);

    let mut open_issues: Vec<&Issue> = Vec::new();
    let mut closed_issues: Vec<&Issue> = Vec::new();
    let mut open_pulls: Vec<&Issue> = Vec::new();
    let mut closed_pulls: Vec<&Issue> = Vec::new();

    let mut issues_opened = TimeWindowStats::default();
    let mut issues_closed = TimeWindowStats::default();
    let mut prs_opened = TimeWindowStats::default();
    let mut prs_merged = TimeWindowStats::default();
    let mut prs_closed = TimeWindowStats::default();

    // Classify issues/PRs and compute windowed counts in a single pass
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

        if is_pr {
            increment_window(&mut prs_opened, issue.created_at, cutoff_90, cutoff_180, cutoff_365);
            if let Some(closed) = issue.closed_at {
                increment_window(&mut prs_closed, closed, cutoff_90, cutoff_180, cutoff_365);
            }
            if let Some(merged) = issue.pull_request.as_ref().and_then(|p| p.merged_at) {
                increment_window(&mut prs_merged, merged, cutoff_90, cutoff_180, cutoff_365);
            }
        } else {
            increment_window(&mut issues_opened, issue.created_at, cutoff_90, cutoff_180, cutoff_365);
            if let Some(closed) = issue.closed_at {
                increment_window(&mut issues_closed, closed, cutoff_90, cutoff_180, cutoff_365);
            }
        }
    }

    // Open ages: duration from creation to now
    let open_issue_age = compute_age_stats(open_issues.iter().map(|i| (now - i.created_at).num_seconds() as f64));
    let open_pr_age = compute_age_stats(open_pulls.iter().map(|i| (now - i.created_at).num_seconds() as f64));

    // Closed issue ages (issues without closed_at are excluded via closed_age_seconds)
    let closed_issue_age = compute_age_stats(closed_issues.iter().copied().filter_map(closed_age_seconds));
    let closed_issue_age_last_90_days = compute_age_stats(
        closed_issues.iter().copied()
            .filter(|i| i.closed_at.is_some_and(|t| t >= cutoff_90))
            .filter_map(closed_age_seconds),
    );
    let closed_issue_age_last_180_days = compute_age_stats(
        closed_issues.iter().copied()
            .filter(|i| i.closed_at.is_some_and(|t| t >= cutoff_180))
            .filter_map(closed_age_seconds),
    );
    let closed_issue_age_last_365_days = compute_age_stats(
        closed_issues.iter().copied()
            .filter(|i| i.closed_at.is_some_and(|t| t >= cutoff_365))
            .filter_map(closed_age_seconds),
    );

    // Merged PR ages (chaining iterators avoids intermediate Vec allocation)
    let all_pulls = || open_pulls.iter().chain(closed_pulls.iter()).copied();
    let merged_pr_age = compute_age_stats(all_pulls().filter_map(merged_pr_age_seconds));
    let merged_pr_age_last_90_days = compute_age_stats(
        all_pulls()
            .filter(|i| i.pull_request.as_ref().and_then(|p| p.merged_at).is_some_and(|t| t >= cutoff_90))
            .filter_map(merged_pr_age_seconds),
    );
    let merged_pr_age_last_180_days = compute_age_stats(
        all_pulls()
            .filter(|i| i.pull_request.as_ref().and_then(|p| p.merged_at).is_some_and(|t| t >= cutoff_180))
            .filter_map(merged_pr_age_seconds),
    );
    let merged_pr_age_last_365_days = compute_age_stats(
        all_pulls()
            .filter(|i| i.pull_request.as_ref().and_then(|p| p.merged_at).is_some_and(|t| t >= cutoff_365))
            .filter_map(merged_pr_age_seconds),
    );

    IssueAndPullStats {
        request_count: 0,
        open_issues: open_issues.len() as u64,
        open_prs: open_pulls.len() as u64,
        issues_opened,
        issues_closed,
        prs_opened,
        prs_merged,
        prs_closed,
        open_issue_age,
        open_pr_age,
        closed_issue_age,
        closed_issue_age_last_90_days,
        closed_issue_age_last_180_days,
        closed_issue_age_last_365_days,
        merged_pr_age,
        merged_pr_age_last_90_days,
        merged_pr_age_last_180_days,
        merged_pr_age_last_365_days,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_compute_age_stats_empty() {
        let stats = compute_age_stats(core::iter::empty());
        assert_eq!(stats.avg, 0);
        assert_eq!(stats.p50, 0);
        assert_eq!(stats.p75, 0);
        assert_eq!(stats.p90, 0);
        assert_eq!(stats.p95, 0);
    }

    #[test]
    fn test_compute_age_stats_open_issues() {
        let seconds_per_day = 86400.0_f64;
        let stats = compute_age_stats([10.0, 20.0, 5.0].iter().map(|&days| days * seconds_per_day));
        // Average of 5, 10, 20 = 11.67 days
        assert!(stats.avg >= 11 && stats.avg <= 12);
        assert!(stats.p50 >= 9 && stats.p50 <= 11);
    }

    #[test]
    fn test_compute_age_stats_closed_issues() {
        let seconds_per_day = 86400.0_f64;
        // First issue was open for 10 days, second for 5 days
        let stats = compute_age_stats([10.0, 5.0].iter().map(|&days| days * seconds_per_day));
        // Average around 7.5 days
        assert!(stats.avg >= 7 && stats.avg <= 8);
    }

    fn test_cache() -> Cache {
        Cache::new("test_cache", Duration::from_secs(3600), false)
    }

    #[test]
    fn test_get_cache_filename() {
        let filename = Provider::get_cache_filename("github.com", "tokio-rs", "tokio");

        assert!(filename.contains("github.com"));
        assert!(filename.contains("tokio-rs"));
        assert!(filename.contains("tokio.json"));
    }

    #[test]
    fn test_get_cache_filename_sanitized() {
        let filename = Provider::get_cache_filename("evil.com", "../../../etc", "passwd");

        // Path traversal should be sanitized
        assert!(!filename.contains("../"));
        assert!(filename.contains("passwd.json"));
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_repo_url() {
        let client = Client::new(None, "https://api.github.com").unwrap();

        let url = Provider::repo_url(&client, "tokio-rs", "tokio", "");
        assert_eq!(url, "https://api.github.com/repos/tokio-rs/tokio");

        let url_with_suffix = Provider::repo_url(&client, "tokio-rs", "tokio", "/commits");
        assert_eq!(url_with_suffix, "https://api.github.com/repos/tokio-rs/tokio/commits");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_repo_data_from_cache() {
        let repo_spec = RepoSpec::parse(&url::Url::parse("https://github.com/tokio-rs/tokio").unwrap()).unwrap();
        let hosting_data = HostingData {
            stars: 1000,
            forks: 200,
            subscribers: 50,
            open_issues: 10,
            open_prs: 5,
            issues_opened: TimeWindowStats::default(),
            issues_closed: TimeWindowStats::default(),
            prs_opened: TimeWindowStats::default(),
            prs_merged: TimeWindowStats::default(),
            prs_closed: TimeWindowStats::default(),
            open_issue_age: AgeStats::default(),
            open_pr_age: AgeStats::default(),
            closed_issue_age: AgeStats::default(),
            closed_issue_age_last_90_days: AgeStats::default(),
            closed_issue_age_last_180_days: AgeStats::default(),
            closed_issue_age_last_365_days: AgeStats::default(),
            merged_pr_age: AgeStats::default(),
            merged_pr_age_last_90_days: AgeStats::default(),
            merged_pr_age_last_180_days: AgeStats::default(),
            merged_pr_age_last_365_days: AgeStats::default(),
        };

        let repo_data = RepoData::from_cache(repo_spec.clone(), ProviderResult::Found(hosting_data));

        assert_eq!(repo_data.repo_spec, repo_spec);
        assert!(matches!(repo_data.result, ProviderResult::Found(_)));
        assert!(!repo_data.is_rate_limited);
        assert!(repo_data.rate_limit.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_repo_data_success() {
        let repo_spec = RepoSpec::parse(&url::Url::parse("https://github.com/tokio-rs/tokio").unwrap()).unwrap();
        let hosting_data = HostingData {
            stars: 1000,
            forks: 200,
            subscribers: 50,
            open_issues: 10,
            open_prs: 5,
            issues_opened: TimeWindowStats::default(),
            issues_closed: TimeWindowStats::default(),
            prs_opened: TimeWindowStats::default(),
            prs_merged: TimeWindowStats::default(),
            prs_closed: TimeWindowStats::default(),
            open_issue_age: AgeStats::default(),
            open_pr_age: AgeStats::default(),
            closed_issue_age: AgeStats::default(),
            closed_issue_age_last_90_days: AgeStats::default(),
            closed_issue_age_last_180_days: AgeStats::default(),
            closed_issue_age_last_365_days: AgeStats::default(),
            merged_pr_age: AgeStats::default(),
            merged_pr_age_last_90_days: AgeStats::default(),
            merged_pr_age_last_180_days: AgeStats::default(),
            merged_pr_age_last_365_days: AgeStats::default(),
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
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_provider_new() {
        let provider = Provider::new(None, None, test_cache()).unwrap();
        assert_eq!(provider.hosts.len(), 2); // GitHub and Codeberg
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_provider_new_with_tokens() {
        let provider = Provider::new(
            Some("github_token"),
            Some("codeberg_token"),
            test_cache(),
        )
        .unwrap();
        assert_eq!(provider.hosts.len(), 2);
    }

    #[test]
    fn test_compute_age_stats_filters_nan_and_negative() {
        let stats = compute_age_stats([f64::NAN, f64::INFINITY, -100.0, 86400.0].into_iter());
        // Only 86400.0 (1 day) should be counted
        assert_eq!(stats.avg, 1);
        assert_eq!(stats.p50, 1);
    }

    #[test]
    fn test_closed_age_seconds_with_closed_at() {
        let created = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().to_utc();
        let closed = DateTime::parse_from_rfc3339("2024-01-02T00:00:00Z").unwrap().to_utc();
        let issue = Issue {
            created_at: created,
            closed_at: Some(closed),
            state: IssueState::Closed,
            pull_request: None,
        };
        let age = closed_age_seconds(&issue).unwrap();
        assert!((age - 86400.0).abs() < 1.0);
    }

    #[test]
    fn test_closed_age_seconds_without_closed_at() {
        let created = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().to_utc();
        let issue = Issue {
            created_at: created,
            closed_at: None,
            state: IssueState::Open,
            pull_request: None,
        };
        assert!(closed_age_seconds(&issue).is_none());
    }

    #[test]
    fn test_merged_pr_age_seconds_merged() {
        use super::super::client::PullRequestMarker;
        let created = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().to_utc();
        let merged = DateTime::parse_from_rfc3339("2024-01-03T00:00:00Z").unwrap().to_utc();
        let issue = Issue {
            created_at: created,
            closed_at: Some(merged),
            state: IssueState::Closed,
            pull_request: Some(PullRequestMarker { merged_at: Some(merged) }),
        };
        let age = merged_pr_age_seconds(&issue).unwrap();
        assert!((age - 172_800.0).abs() < 1.0); // 2 days
    }

    #[test]
    fn test_merged_pr_age_seconds_not_merged() {
        use super::super::client::PullRequestMarker;
        let created = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().to_utc();
        let issue = Issue {
            created_at: created,
            closed_at: None,
            state: IssueState::Open,
            pull_request: Some(PullRequestMarker { merged_at: None }),
        };
        assert!(merged_pr_age_seconds(&issue).is_none());
    }

    #[test]
    fn test_merged_pr_age_seconds_not_a_pr() {
        let created = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z").unwrap().to_utc();
        let issue = Issue {
            created_at: created,
            closed_at: None,
            state: IssueState::Open,
            pull_request: None,
        };
        assert!(merged_pr_age_seconds(&issue).is_none());
    }

    #[test]
    fn test_increment_window_recent() {
        let now = Utc::now();
        let cutoff_90 = now - chrono::Duration::days(90);
        let cutoff_180 = now - chrono::Duration::days(180);
        let cutoff_365 = now - chrono::Duration::days(365);

        let mut stats = TimeWindowStats::default();
        // Timestamp within last 90 days
        increment_window(&mut stats, now - chrono::Duration::days(10), cutoff_90, cutoff_180, cutoff_365);
        assert_eq!(stats.total, 1);
        assert_eq!(stats.last_90_days, 1);
        assert_eq!(stats.last_180_days, 1);
        assert_eq!(stats.last_365_days, 1);
    }

    #[test]
    fn test_increment_window_old() {
        let now = Utc::now();
        let cutoff_90 = now - chrono::Duration::days(90);
        let cutoff_180 = now - chrono::Duration::days(180);
        let cutoff_365 = now - chrono::Duration::days(365);

        let mut stats = TimeWindowStats::default();
        // Timestamp between 180 and 365 days ago
        increment_window(&mut stats, now - chrono::Duration::days(200), cutoff_90, cutoff_180, cutoff_365);
        assert_eq!(stats.total, 1);
        assert_eq!(stats.last_90_days, 0);
        assert_eq!(stats.last_180_days, 0);
        assert_eq!(stats.last_365_days, 1);
    }

    #[test]
    fn test_increment_window_very_old() {
        let now = Utc::now();
        let cutoff_90 = now - chrono::Duration::days(90);
        let cutoff_180 = now - chrono::Duration::days(180);
        let cutoff_365 = now - chrono::Duration::days(365);

        let mut stats = TimeWindowStats::default();
        // Timestamp older than 365 days
        increment_window(&mut stats, now - chrono::Duration::days(400), cutoff_90, cutoff_180, cutoff_365);
        assert_eq!(stats.total, 1);
        assert_eq!(stats.last_90_days, 0);
        assert_eq!(stats.last_180_days, 0);
        assert_eq!(stats.last_365_days, 0);
    }

    #[test]
    fn test_compute_all_stats_empty() {
        let now = Utc::now();
        let stats = compute_all_stats(&[], now);
        assert_eq!(stats.open_issues, 0);
        assert_eq!(stats.open_prs, 0);
        assert_eq!(stats.issues_opened.total, 0);
        assert_eq!(stats.prs_opened.total, 0);
    }

    #[test]
    fn test_compute_all_stats_mixed_issues_and_prs() {
        use super::super::client::PullRequestMarker;
        let now = Utc::now();
        let day_ago = now - chrono::Duration::days(1);
        let week_ago = now - chrono::Duration::days(7);
        let two_days_ago = now - chrono::Duration::days(2);

        let issues = vec![
            // Open issue
            Issue {
                created_at: week_ago,
                closed_at: None,
                state: IssueState::Open,
                pull_request: None,
            },
            // Closed issue
            Issue {
                created_at: week_ago,
                closed_at: Some(day_ago),
                state: IssueState::Closed,
                pull_request: None,
            },
            // Open PR
            Issue {
                created_at: two_days_ago,
                closed_at: None,
                state: IssueState::Open,
                pull_request: Some(PullRequestMarker { merged_at: None }),
            },
            // Merged PR
            Issue {
                created_at: week_ago,
                closed_at: Some(two_days_ago),
                state: IssueState::Closed,
                pull_request: Some(PullRequestMarker { merged_at: Some(two_days_ago) }),
            },
        ];

        let stats = compute_all_stats(&issues, now);
        assert_eq!(stats.open_issues, 1);
        assert_eq!(stats.open_prs, 1);
        assert_eq!(stats.issues_opened.total, 2);
        assert_eq!(stats.issues_closed.total, 1);
        assert_eq!(stats.prs_opened.total, 2);
        assert_eq!(stats.prs_merged.total, 1);
        assert_eq!(stats.prs_closed.total, 1);
    }

    #[test]
    fn test_percentile_boundary_values() {
        let data = vec![1.0, 2.0, 3.0];
        assert!((percentile(&data, 0.0) - 1.0).abs() < f64::EPSILON);
        assert!((percentile(&data, 100.0) - 3.0).abs() < f64::EPSILON);
    }
}
