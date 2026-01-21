use crate::Result;
use crate::facts::ProviderResult;
use crate::facts::cache_doc;
use crate::facts::crate_spec::{self, CrateSpec};
use crate::facts::hosting::{AgeStats, HostingData, IssueStats};
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::repo_spec::RepoSpec;
use crate::facts::request_tracker::RequestTracker;
use chrono::Utc;
use core::time::Duration;
use futures_util::future::join_all;
use octocrab::{Octocrab, models::issues::Issue};
use ohno::{EnrichableExt, IntoAppError};
use reqwest::Client;
use reqwest::header::LINK;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};

const LOG_TARGET: &str = "   hosting";
const SECONDS_PER_DAY: f64 = 86400.0;
const COMMIT_LOOKBACK_DAYS: i64 = 90;
const ISSUE_LOOKBACK_DAYS: i64 = 365 * 10;
const ISSUE_PAGE_SIZE: u8 = 100;
const MAX_RATE_LIMIT_WAIT_SECS: u64 = 3600;

const GITHUB_RATE_LIMIT_UNAUTHENTICATED: usize = 60;
const GITHUB_RATE_LIMIT_AUTHENTICATED: usize = 5000;

/// Estimated number of GitHub API requests per repository.
/// Each repo requires at least 4 requests (repo info, contributors, commits, issues page 1),
/// plus additional requests for paginated issues/PRs. Using 10 as a conservative estimate.
const ESTIMATED_REQUESTS_PER_REPO: usize = 10;

/// Pattern to extract page number from GitHub API Link header
static PAGE_REGEX: LazyLock<regex::Regex> = LazyLock::new(|| regex::Regex::new(r"page=(\d+)>; rel=.last.").expect("invalid regex"));

#[derive(Debug, Clone)]
pub struct Provider {
    octocrab: Octocrab,
    client: Client,
    /// Maximum number of concurrent requests to GitHub API.
    /// Set to rate limit + 1 to provide a small buffer for batch processing.
    batch_size: usize,
    cache_dir: Arc<Path>,
    cache_ttl: Duration,
}

impl Provider {
    /// Create a new GitHub API client
    pub fn new(token: Option<&str>, cache_dir: impl AsRef<Path>, cache_ttl: Duration) -> Result<Self> {
        let mut builder = Octocrab::builder();
        let mut client_builder = Client::builder().user_agent("cargo-rank");
        let has_token = token.is_some();

        if let Some(t) = token {
            let mut auth_val = reqwest::header::HeaderValue::from_str(&format!("token {t}"))?;
            auth_val.set_sensitive(true);

            let mut headers = reqwest::header::HeaderMap::new();
            let _ = headers.insert(reqwest::header::AUTHORIZATION, auth_val);

            client_builder = client_builder.default_headers(headers);

            builder = builder.personal_token(t);
        }

        Ok(Self {
            octocrab: builder.build()?,
            client: client_builder.build()?,
            batch_size: if has_token {
                (GITHUB_RATE_LIMIT_AUTHENTICATED + 1) / ESTIMATED_REQUESTS_PER_REPO
            } else {
                (GITHUB_RATE_LIMIT_UNAUTHENTICATED + 1) / ESTIMATED_REQUESTS_PER_REPO
            },
            cache_dir: Arc::from(cache_dir.as_ref()),
            cache_ttl,
        })
    }

    /// Get hosting data for multiple crates, deduplicating by repository
    pub async fn get_hosting_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
        tracker: &RequestTracker,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<HostingData>)> {
        let mut repo_to_crates = crate_spec::by_repo(crates);

        // Collect all repositories that need to be fetched and add them to the tracker upfront
        let mut pending_repos: Vec<Arc<RepoSpec>> = repo_to_crates.keys().map(Arc::clone).collect();
        tracker.add_many_requests("github", pending_repos.len() as u64);

        let mut results: Vec<(Arc<RepoSpec>, ProviderResult<HostingData>)> = Vec::new();

        // Process repositories in batches to respect rate limits
        while !pending_repos.is_empty() {
            let batch_size = pending_repos.len().min(self.batch_size);
            let batch: Vec<_> = pending_repos.drain(..batch_size).collect();

            // Fetch all repositories in this batch concurrently
            let batch_futures = batch.iter().map(|repo_spec| self.fetch_hosting_data_for_repo(repo_spec));

            let batch_results: Vec<_> = join_all(batch_futures).await;

            // Check for rate limit errors and collect successful results
            let mut rate_limit_detected = false;
            let mut failed_indices = Vec::new();

            for (i, result) in batch_results.iter().enumerate() {
                if let ProviderResult::Error(e) = result {
                    let error_msg = format!("{e:#}");
                    if error_msg.contains("rate limit") || error_msg.contains("API rate limit exceeded") {
                        rate_limit_detected = true;
                        failed_indices.push(i);
                        // Don't complete - will retry
                    } else {
                        // Non-rate-limit error - add to results as error and mark complete
                        results.push((Arc::clone(&batch[i]), result.clone()));
                        tracker.complete_request("github");
                    }
                } else {
                    // Success - add to results and mark complete
                    results.push((Arc::clone(&batch[i]), result.clone()));
                    tracker.complete_request("github");
                }
            }

            // If we hit rate limit, get actual reset time from GitHub and wait
            if rate_limit_detected {
                // Try to get the actual rate limit reset time from GitHub's API
                let wait_time = if let Ok(rate_limit) = self.octocrab.ratelimit().get().await {
                    log::info!(target: LOG_TARGET, "GitHub rate limit info: remaining={}, reset={}",
                        rate_limit.rate.remaining, rate_limit.rate.reset);
                    rate_limit.rate.reset
                } else {
                    // Fallback: calculate wait time as next hour boundary + 1 minute
                    #[expect(clippy::cast_sign_loss, reason = "timestamp is positive")]
                    let now = Utc::now().timestamp() as u64;
                    let seconds_into_hour = now % 3600;
                    let seconds_until_next_hour = 3600 - seconds_into_hour;
                    now + seconds_until_next_hour + 60
                };

                // Cap to maximum to prevent unreasonably long waits
                #[expect(clippy::cast_sign_loss, reason = "timestamp is positive")]
                let now = Utc::now().timestamp() as u64;
                let wait_time = wait_time.min(now + MAX_RATE_LIMIT_WAIT_SECS);

                #[expect(clippy::cast_possible_wrap, reason = "timestamp is within valid i64 range")]
                let reset_time = chrono::DateTime::from_timestamp(wait_time as i64, 0).expect("valid timestamp");
                let formatted_time = reset_time.with_timezone(&chrono::Local).format("%T");

                eprintln!("GitHub rate limit exceeded: Waiting until {formatted_time}...");

                if wait_time > now {
                    tokio::time::sleep(Duration::from_secs(wait_time - now)).await;
                }

                // Re-add failed repos back to pending for retry
                for &i in &failed_indices {
                    pending_repos.push(Arc::clone(&batch[i]));
                }
            }
        }

        // Map results back to crates
        results.into_iter().flat_map(move |(repo_spec, provider_result)| {
            let crate_specs = repo_to_crates.remove(&repo_spec).expect("repo_spec must exist");
            crate_specs.into_iter().map(move |crate_spec| (crate_spec, provider_result.clone()))
        })
    }

    /// Fetch GitHub repository data for a single repository
    async fn fetch_hosting_data_for_repo(&self, repo_spec: &RepoSpec) -> ProviderResult<HostingData> {
        let owner = repo_spec.owner();
        let repo = repo_spec.repo();

        let cache_path = self.get_cache_path(owner, repo);
        if let Some(data) = cache_doc::load_with_ttl(
            &cache_path,
            self.cache_ttl,
            |data: &HostingData| data.timestamp,
            format!("hosting data for repository '{repo_spec}'"),
        ) {
            return ProviderResult::Found(data);
        }

        log::info!(target: LOG_TARGET, "Querying GitHub for hosting information on repository '{repo_spec}'");

        let (repo_res, contributors_res, commits_res, issues_res) = tokio::join!(
            self.get_repo_info(owner, repo),
            self.get_contributors_count(owner, repo),
            self.get_commits_count(owner, repo),
            self.get_issues_and_pulls(owner, repo)
        );

        let repo_data = match repo_res {
            Ok(data) => data,
            Err(e) => {
                if let Some(octocrab::Error::GitHub { source, .. }) = e.source().and_then(|e| e.downcast_ref::<octocrab::Error>())
                    && source.status_code.as_u16() == 404
                {
                    log::info!(target: LOG_TARGET, "Repository '{repo_spec}' not found (404)");
                    return ProviderResult::CrateNotFound;
                }
                log::info!(target: LOG_TARGET, "Failed to fetch repo info for '{repo_spec}': {e:#}");
                return ProviderResult::Error(Arc::new(
                    e.enrich_with(|| format!("could not fetch repo info for repository '{repo_spec}'")),
                ));
            }
        };

        let (issues, pulls) = match issues_res {
            Ok(data) => data,
            Err(e) => {
                log::info!(target: LOG_TARGET, "Failed to fetch issues/PRs for '{repo_spec}': {e:#}");
                return ProviderResult::Error(Arc::new(
                    e.enrich_with(|| format!("could not fetch issues and pull request info for repository '{repo_spec}'")),
                ));
            }
        };

        let contributors = match contributors_res {
            Ok(data) => data,
            Err(e) => {
                log::info!(target: LOG_TARGET, "Failed to fetch contributors for '{repo_spec}': {e:#}");
                return ProviderResult::Error(Arc::new(
                    e.enrich_with(|| format!("could not fetch contributor count for repository '{repo_spec}'")),
                ));
            }
        };

        let commits = match commits_res {
            Ok(data) => data,
            Err(e) => {
                log::info!(target: LOG_TARGET, "Failed to fetch commits for '{repo_spec}': {e:#}");
                return ProviderResult::Error(Arc::new(
                    e.enrich_with(|| format!("could not fetch commit count for repository '{repo_spec}'")),
                ));
            }
        };

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

        match cache_doc::save(&hosting_data, &cache_path) {
            Ok(()) => ProviderResult::Found(hosting_data),
            Err(e) => ProviderResult::Error(Arc::new(e)),
        }
    }

    /// Get the cache file path for a specific repository
    fn get_cache_path(&self, owner: &str, repo: &str) -> PathBuf {
        let safe_owner = sanitize_path_component(owner);
        let safe_repo = sanitize_path_component(repo);
        self.cache_dir.join(&safe_owner).join(format!("{safe_repo}.json"))
    }

    async fn get_count_via_link_header(&self, url: &str) -> Result<u64> {
        log::debug!(target: LOG_TARGET, "Fetching count via Link header from '{url}'");

        let resp = self.client.get(url).send().await?;

        if let Some(link_header) = resp.headers().get(LINK) {
            let link_str = link_header.to_str()?;
            if let Some(count) = PAGE_REGEX.captures(link_str).and_then(|caps| caps.get(1)) {
                log::debug!(target: LOG_TARGET, "Fetched count via Link header from '{url}'");
                return Ok(count.as_str().parse()?);
            }
        }

        // Download response as bytes and parse
        let bytes = resp
            .bytes()
            .await
            .into_app_err_with(|| format!("could not read response body from '{url}'"))?;

        log::debug!(target: LOG_TARGET, "Fetched response from '{url}'");

        Self::count_json_array_elements(&bytes).into_app_err_with(|| format!("could not count items in JSON response from '{url}'"))
    }

    /// Count elements in a JSON array without allocating parsed values.
    /// Uses `IgnoredAny` to skip deserialization of element contents, only counting them.
    /// This is memory-efficient for counting without needing the actual data.
    fn count_json_array_elements(json: &[u8]) -> Result<u64> {
        use serde::de::IgnoredAny;

        let array: Vec<IgnoredAny> = serde_json::from_slice(json).into_app_err("malformed JSON while counting array elements")?;

        Ok(array.len() as u64)
    }

    async fn get_repo_info(&self, owner: &str, repo: &str) -> Result<octocrab::models::Repository> {
        Ok(self.octocrab.repos(owner, repo).get().await?)
    }

    async fn get_contributors_count(&self, owner: &str, repo: &str) -> Result<u64> {
        let url = format!("https://api.github.com/repos/{owner}/{repo}/contributors?per_page=1&anon=true");
        self.get_count_via_link_header(&url).await
    }

    async fn get_commits_count(&self, owner: &str, repo: &str) -> Result<u64> {
        let since = Utc::now() - chrono::Duration::days(COMMIT_LOOKBACK_DAYS);
        let since_str = since.to_rfc3339();
        let url = format!("https://api.github.com/repos/{owner}/{repo}/commits?since={since_str}&per_page=1");
        self.get_count_via_link_header(&url).await
    }

    async fn get_issues_and_pulls(&self, owner: &str, repo: &str) -> Result<(IssueStats, IssueStats)> {
        let since = Utc::now() - chrono::Duration::days(ISSUE_LOOKBACK_DAYS);

        log::debug!(target: LOG_TARGET, "Fetching issues and pull requests for '{owner}/{repo}'");

        let mut page = self
            .octocrab
            .issues(owner, repo)
            .list()
            .state(octocrab::params::State::All)
            .since(since)
            .per_page(ISSUE_PAGE_SIZE)
            .send()
            .await?;

        log::debug!(target: LOG_TARGET, "Fetched issues and pull requests for '{owner}/{repo}'");

        let mut all_issues = page.take_items();

        while let Some(next_uri) = &page.next {
            let next_page_result = self.octocrab.get_page::<Issue>(&Some(next_uri.clone())).await?;

            if let Some(mut next_page) = next_page_result {
                all_issues.append(&mut next_page.take_items());
                page = next_page;
            } else {
                break;
            }
        }

        // Pre-allocate vectors with rough capacity estimate to reduce reallocations
        let total = all_issues.len();
        let mut open_issues = Vec::with_capacity(total / 4);
        let mut closed_issues = Vec::with_capacity(total / 4);
        let mut open_pulls = Vec::with_capacity(total / 4);
        let mut closed_pulls = Vec::with_capacity(total / 4);

        for issue in all_issues {
            let is_pr = issue.pull_request.is_some();
            let is_open = issue.state == octocrab::models::IssueState::Open;

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

        let issues_stats = IssueStats {
            open_count: open_issues.len() as u64,
            closed_count: closed_issues.len() as u64,
            open_age: Self::compute_age(&open_issues, true),
            closed_age: Self::compute_age(&closed_issues, false),
        };

        let pulls_stats = IssueStats {
            open_count: open_pulls.len() as u64,
            closed_count: closed_pulls.len() as u64,
            open_age: Self::compute_age(&open_pulls, true),
            closed_age: Self::compute_age(&closed_pulls, false),
        };

        Ok((issues_stats, pulls_stats))
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
            p50: (Self::percentile(&seconds, 50.0) / SECONDS_PER_DAY) as u32,
            p75: (Self::percentile(&seconds, 75.0) / SECONDS_PER_DAY) as u32,
            p90: (Self::percentile(&seconds, 90.0) / SECONDS_PER_DAY) as u32,
            p95: (Self::percentile(&seconds, 95.0) / SECONDS_PER_DAY) as u32,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_json_array_elements() {
        // Empty array
        assert_eq!(Provider::count_json_array_elements(b"[]").unwrap(), 0);

        // Single element
        assert_eq!(Provider::count_json_array_elements(br#"[{"id": 1}]"#).unwrap(), 1);

        // Multiple elements
        assert_eq!(
            Provider::count_json_array_elements(br#"[{"id": 1}, {"id": 2}, {"id": 3}]"#).unwrap(),
            3
        );

        // Complex objects (like GitHub contributors)
        let json = br#"[
            {"login": "user1", "contributions": 100},
            {"login": "user2", "contributions": 50},
            {"login": "user3", "contributions": 25}
        ]"#;
        assert_eq!(Provider::count_json_array_elements(json).unwrap(), 3);

        // Malformed JSON should error
        let _ = Provider::count_json_array_elements(b"[{broken").unwrap_err();
    }
}
