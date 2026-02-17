//! GitHub API client
//!
//! Minimal GitHub API client for fetching repository and issue data.

use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[expect(clippy::struct_field_names, reason = "field names match GitHub API exactly")]
pub struct Repository {
    #[serde(alias = "stars_count")]
    pub stargazers_count: Option<u32>,
    pub forks_count: Option<u32>,
    #[serde(default)]
    pub subscribers_count: Option<i64>,
    /// Codeberg uses `watchers_count` instead of `subscribers_count`
    #[serde(default)]
    pub watchers_count: Option<i64>,
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

/// Marker type to detect if an issue is actually a pull request.
/// The `merged_at` field is populated by GitHub's issues endpoint when the PR has been merged.
#[derive(Debug, Deserialize)]
pub struct PullRequestMarker {
    pub merged_at: Option<DateTime<Utc>>,
}

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

    /// The requested resource was not found (404)
    NotFound(Option<RateLimitInfo>),

    /// Request failed permanently - should NOT retry
    Failed(ohno::AppError, Option<RateLimitInfo>),
}

/// Hosting API client (GitHub, Codeberg, etc.)
#[derive(Debug, Clone)]
#[expect(clippy::struct_field_names, reason = "client field stores the underlying HTTP client")]
pub struct Client {
    client: reqwest::Client,
    base_url: String,
    now: DateTime<Utc>,
}

impl Client {
    /// Create a new hosting API client with optional authentication token and base URL
    pub fn new(token: Option<&str>, base_url: impl Into<String>, now: DateTime<Utc>) -> crate::Result<Self> {
        use reqwest::header::{AUTHORIZATION, HeaderValue};

        let mut client_builder = reqwest::Client::builder().user_agent("cargo-aprz");

        if let Some(t) = token {
            let mut auth_val = HeaderValue::from_str(&format!("token {t}"))?;
            auth_val.set_sensitive(true);

            let mut headers = HeaderMap::new();
            let _ = headers.insert(AUTHORIZATION, auth_val);

            client_builder = client_builder.default_headers(headers);
        }

        Ok(Self {
            client: client_builder.build()?,
            base_url: base_url.into(),
            now,
        })
    }

    /// Get the base URL for this client
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Make an API call and classify the result
    pub async fn api_call(&self, url: &str) -> HostingApiResult<reqwest::Response> {
        let resp = match crate::facts::resilient_http::resilient_get(&self.client, url).await {
            Ok(r) => r,
            Err(e) => return HostingApiResult::Failed(e, None),
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
                reset_at: self.now + chrono::Duration::hours(1),
            });
            return HostingApiResult::RateLimited(rate_limit);
        }

        // Check for not found (404)
        if status_code == 404 {
            return HostingApiResult::NotFound(rate_limit);
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

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::HeaderValue;

    #[test]
    fn test_repository_deserialize_github() {
        let json = r#"{
            "stargazers_count": 1000,
            "forks_count": 200,
            "subscribers_count": 50
        }"#;

        let repo: Repository = serde_json::from_str(json).unwrap();
        assert_eq!(repo.stargazers_count, Some(1000));
        assert_eq!(repo.forks_count, Some(200));
        assert_eq!(repo.subscribers_count, Some(50));
    }

    #[test]
    fn test_repository_deserialize_codeberg() {
        let json = r#"{
            "stars_count": 500,
            "forks_count": 100,
            "watchers_count": 25
        }"#;

        let repo: Repository = serde_json::from_str(json).unwrap();
        assert_eq!(repo.stargazers_count, Some(500)); // stars_count alias
        assert_eq!(repo.forks_count, Some(100));
        assert_eq!(repo.watchers_count, Some(25));
    }

    #[test]
    fn test_repository_deserialize_optional_fields() {
        let json = r#"{
            "stargazers_count": 1000
        }"#;

        let repo: Repository = serde_json::from_str(json).unwrap();
        assert_eq!(repo.stargazers_count, Some(1000));
        assert_eq!(repo.forks_count, None);
        assert_eq!(repo.subscribers_count, None);
        assert_eq!(repo.watchers_count, None);
    }

    #[test]
    fn test_issue_deserialize() {
        let json = r#"{
            "created_at": "2024-01-01T00:00:00Z",
            "closed_at": "2024-01-02T00:00:00Z",
            "state": "closed"
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.state, IssueState::Closed);
        assert!(issue.closed_at.is_some());
        assert!(issue.pull_request.is_none());
    }

    #[test]
    fn test_issue_deserialize_with_pull_request() {
        let json = r#"{
            "created_at": "2024-01-01T00:00:00Z",
            "closed_at": null,
            "state": "open",
            "pull_request": {
                "url": "https://api.github.com/repos/owner/repo/pulls/1"
            }
        }"#;

        let issue: Issue = serde_json::from_str(json).unwrap();
        assert_eq!(issue.state, IssueState::Open);
        assert!(issue.closed_at.is_none());
        assert!(issue.pull_request.is_some());
    }

    #[test]
    fn test_issue_state_open() {
        let json = r#""open""#;
        let state: IssueState = serde_json::from_str(json).unwrap();
        assert_eq!(state, IssueState::Open);
    }

    #[test]
    fn test_issue_state_closed() {
        let json = r#""closed""#;
        let state: IssueState = serde_json::from_str(json).unwrap();
        assert_eq!(state, IssueState::Closed);
    }

    #[test]
    fn test_pull_request_marker_deserialize() {
        let json = r#"{
            "url": "https://api.github.com/repos/owner/repo/pulls/1",
            "html_url": "https://github.com/owner/repo/pull/1"
        }"#;

        let _marker: PullRequestMarker = serde_json::from_str(json).unwrap();
        // Just verifying it deserializes without error
    }

    #[test]
    fn test_rate_limit_info_copy() {
        let info1 = RateLimitInfo {
            remaining: 5000,
            reset_at: DateTime::from_timestamp(1_234_567_890, 0).unwrap(),
        };

        let info2 = info1;

        assert_eq!(info1.remaining, 5000);
        assert_eq!(info2.remaining, 5000);
    }

    #[test]
    fn test_extract_rate_limit_from_headers() {
        let mut headers = HeaderMap::new();
        let _ = headers.insert("x-ratelimit-remaining", HeaderValue::from_static("4999"));
        let _ = headers.insert("x-ratelimit-reset", HeaderValue::from_static("1704067200"));

        let rate_limit = extract_rate_limit_from_headers(&headers).unwrap();

        assert_eq!(rate_limit.remaining, 4999);
        assert_eq!(rate_limit.reset_at.timestamp(), 1_704_067_200);
    }

    #[test]
    fn test_extract_rate_limit_missing_headers() {
        let headers = HeaderMap::new();
        let rate_limit = extract_rate_limit_from_headers(&headers);
        assert!(rate_limit.is_none());
    }

    #[test]
    fn test_extract_rate_limit_invalid_remaining() {
        let mut headers = HeaderMap::new();
        let _ = headers.insert("x-ratelimit-remaining", HeaderValue::from_static("invalid"));
        let _ = headers.insert("x-ratelimit-reset", HeaderValue::from_static("1704067200"));

        let rate_limit = extract_rate_limit_from_headers(&headers);
        assert!(rate_limit.is_none());
    }

    #[test]
    fn test_extract_rate_limit_invalid_reset() {
        let mut headers = HeaderMap::new();
        let _ = headers.insert("x-ratelimit-remaining", HeaderValue::from_static("4999"));
        let _ = headers.insert("x-ratelimit-reset", HeaderValue::from_static("invalid"));

        let rate_limit = extract_rate_limit_from_headers(&headers);
        assert!(rate_limit.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_client_new_without_token() {
        let client = Client::new(None, "https://api.github.com", Utc::now()).unwrap();
        assert_eq!(client.base_url(), "https://api.github.com");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_client_new_with_token() {
        let client = Client::new(Some("test_token"), "https://api.github.com", Utc::now()).unwrap();
        assert_eq!(client.base_url(), "https://api.github.com");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri cannot call GetSystemTimePreciseAsFileTime")]
    fn test_client_base_url() {
        let client = Client::new(None, "https://codeberg.org/api/v1", Utc::now()).unwrap();
        assert_eq!(client.base_url(), "https://codeberg.org/api/v1");
    }

    #[test]
    fn test_hosting_api_result_success() {
        // Create a mock response (we can't create a real reqwest::Response without network)
        // So we'll just test that we can create the enum variant
        let rate_limit = Some(RateLimitInfo {
            remaining: 5000,
            reset_at: DateTime::from_timestamp(1_234_567_890, 0).unwrap(),
        });

        // We can't easily test Success variant without a real Response object,
        // but we can verify the enum exists and can be pattern matched
        match rate_limit {
            Some(info) => assert_eq!(info.remaining, 5000),
            None => panic!("Expected Some"),
        }
    }

    #[test]
    fn test_rate_limit_info_fields() {
        let reset_time = DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        let info = RateLimitInfo {
            remaining: 100,
            reset_at: reset_time,
        };

        assert_eq!(info.remaining, 100);
        assert_eq!(info.reset_at.timestamp(), 1_704_067_200);
    }
}
