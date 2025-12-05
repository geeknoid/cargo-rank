//! GitHub API client
//!
//! Minimal GitHub API client for fetching repository and issue data.

use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use serde::Deserialize;

/// Minimal GitHub repository info with only the fields we need
#[derive(Debug, Deserialize)]
#[expect(clippy::struct_field_names, reason = "field names match GitHub API exactly")]
pub struct Repository {
    pub stargazers_count: Option<u32>,
    pub forks_count: Option<u32>,
    pub subscribers_count: Option<i64>,
}

/// Minimal GitHub issue/PR info with only the fields we need
#[derive(Debug, Deserialize)]
pub struct Issue {
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub state: IssueState,
    pub pull_request: Option<PullRequestMarker>,
}

/// Issue state: open or closed
#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum IssueState {
    Open,
    Closed,
}

/// Marker type to detect if an issue is actually a pull request
/// We don't need any fields, just presence/absence
#[derive(Debug, Deserialize)]
pub struct PullRequestMarker;

/// Rate limit information from response headers
#[derive(Debug, Clone, Copy)]
pub struct RateLimitInfo {
    pub remaining: usize,
    pub reset_at: DateTime<Utc>,
}

/// Result of a hosting API call
pub enum HostingApiResult<T> {
    /// Request succeeded - contains data and optional rate limit info
    Success(T, Option<RateLimitInfo>),

    /// Rate limited - should retry after reset time
    RateLimited(RateLimitInfo),

    /// Request failed permanently - should NOT retry
    Failed(ohno::AppError, Option<RateLimitInfo>),
}

/// GitHub API client
#[derive(Debug, Clone)]
pub struct Client {
    client: reqwest::Client,
}

impl Client {
    /// Create a new GitHub API client with optional authentication token
    pub fn new(token: Option<&str>) -> crate::Result<Self> {
        use reqwest::header::{AUTHORIZATION, HeaderValue};

        let mut client_builder = reqwest::Client::builder().user_agent("cargo-rank");

        if let Some(t) = token {
            let mut auth_val = HeaderValue::from_str(&format!("token {t}"))?;
            auth_val.set_sensitive(true);

            let mut headers = HeaderMap::new();
            let _ = headers.insert(AUTHORIZATION, auth_val);

            client_builder = client_builder.default_headers(headers);
        }

        Ok(Self {
            client: client_builder.build()?,
        })
    }

    /// Make an API call and classify the result
    pub async fn api_call(&self, url: &str) -> HostingApiResult<reqwest::Response> {
        let resp = match self.client.get(url).send().await {
            Ok(r) => r,
            Err(e) => return HostingApiResult::Failed(e.into(), None),
        };

        // Extract rate limit info from response headers before checking status
        let rate_limit = extract_rate_limit_from_headers(resp.headers());

        // Check status code
        let status = resp.status();
        if status.is_success() {
            return HostingApiResult::Success(resp, rate_limit);
        }

        // Check for rate limiting (403 or 429)
        let status_code = status.as_u16();
        if matches!(status_code, 403 | 429) {
            // Rate limited - use rate limit info from headers or default to 1 hour retry
            let rate_limit = rate_limit.unwrap_or_else(|| RateLimitInfo {
                remaining: 0,
                reset_at: Utc::now() + chrono::Duration::hours(1),
            });
            return HostingApiResult::RateLimited(rate_limit);
        }

        // Any other HTTP error is a permanent failure
        let error = resp.error_for_status().expect_err("status is not successful at this point");
        HostingApiResult::Failed(error.into(), rate_limit)
    }
}

/// Extract rate limit information from API response headers
fn extract_rate_limit_from_headers(headers: &HeaderMap) -> Option<RateLimitInfo> {
    let remaining = headers.get("x-ratelimit-remaining")?.to_str().ok()?.parse::<usize>().ok()?;

    let reset_timestamp = headers.get("x-ratelimit-reset")?.to_str().ok()?.parse::<i64>().ok()?;

    let reset_at = DateTime::from_timestamp(reset_timestamp, 0)?;

    Some(RateLimitInfo { remaining, reset_at })
}
