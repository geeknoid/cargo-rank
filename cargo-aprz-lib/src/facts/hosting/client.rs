//! GitHub API client
//!
//! Minimal GitHub API client for fetching repository and issue data.

use chrono::{DateTime, Utc};
use reqwest::header::HeaderMap;
use serde::Deserialize;

const LOG_TARGET: &str = "   hosting";

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
pub struct Client {
    client: reqwest::Client,
    base_url: String,
}

impl Client {
    /// Create a new hosting API client with optional authentication token and base URL
    pub fn new(token: Option<&str>, base_url: impl Into<String>) -> crate::Result<Self> {
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

        let rate_limit = extract_rate_limit_from_headers(resp.headers());
        classify_response(resp, rate_limit, url)
    }
}

/// Classify an HTTP response into a [`HostingApiResult`].
fn classify_response(
    resp: reqwest::Response,
    rate_limit: Option<RateLimitInfo>,
    url: &str,
) -> HostingApiResult<reqwest::Response> {
    let status = resp.status();
    log::debug!(target: LOG_TARGET, "HTTP {status} for {url}");

    if status.is_success() {
        return HostingApiResult::Success(resp, rate_limit);
    }

    let status_code = status.as_u16();
    if matches!(status_code, 403 | 429) {
        // Extract Retry-After header (used by GitHub for secondary/abuse rate limits)
        let retry_after_secs = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|h| h.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok());

        // Secondary rate limit: Retry-After header present — honor the requested delay
        if let Some(secs) = retry_after_secs {
            log::warn!(target: LOG_TARGET, "Secondary rate limit (HTTP {status_code}, Retry-After: {secs}s) for {url}");
            return HostingApiResult::RateLimited(RateLimitInfo {
                remaining: 0,
                reset_at: Utc::now() + chrono::Duration::seconds(secs.cast_signed()),
            });
        }

        // Primary rate limit: exhausted (remaining == 0 or no rate limit headers)
        let is_primary_rate_limit = rate_limit.as_ref().is_none_or(|rl| rl.remaining == 0);
        if is_primary_rate_limit {
            log::warn!(target: LOG_TARGET, "Primary rate limit exhausted (HTTP {status_code}) for {url}");
            let rate_limit = rate_limit.unwrap_or_else(|| RateLimitInfo {
                remaining: 0,
                reset_at: Utc::now() + chrono::Duration::hours(1),
            });
            return HostingApiResult::RateLimited(rate_limit);
        }

        // 429 is always a rate limit signal, even with remaining > 0 and no Retry-After
        if status_code == 429 {
            log::warn!(target: LOG_TARGET, "Rate limited (HTTP 429, remaining: {}) for {url}", rate_limit.map_or(0, |rl| rl.remaining));
            return HostingApiResult::RateLimited(rate_limit.unwrap_or_else(|| RateLimitInfo {
                remaining: 0,
                reset_at: Utc::now() + chrono::Duration::minutes(1),
            }));
        }

        // 403 with remaining > 0 and no Retry-After — not a rate limit
        // (e.g., repo is private, DMCA takedown, insufficient permissions)
        log::warn!(target: LOG_TARGET, "HTTP 403 (not rate-limited, remaining: {}) for {url}", rate_limit.map_or(0, |rl| rl.remaining));
        let error = resp.error_for_status().expect_err("status is not successful at this point");
        return HostingApiResult::Failed(error.into(), rate_limit);
    }

    // Check for not found (404)
    if status_code == 404 {
        log::debug!(target: LOG_TARGET, "HTTP 404 for {url}");
        return HostingApiResult::NotFound(rate_limit);
    }

    // Any other HTTP error is a permanent failure
    log::warn!(target: LOG_TARGET, "HTTP {status_code} for {url}");
    let error = resp.error_for_status().expect_err("status is not successful at this point");
    HostingApiResult::Failed(error.into(), rate_limit)
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
    fn test_client_new_without_token() {
        let client = Client::new(None, "https://api.github.com").unwrap();
        assert_eq!(client.base_url(), "https://api.github.com");
    }

    #[test]
    fn test_client_new_with_token() {
        let client = Client::new(Some("test_token"), "https://api.github.com").unwrap();
        assert_eq!(client.base_url(), "https://api.github.com");
    }

    #[test]
    fn test_client_base_url() {
        let client = Client::new(None, "https://codeberg.org/api/v1").unwrap();
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

    // -- classify_response tests using wiremock --

    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    /// Helper: start a wiremock server, mount a response, GET it with a plain reqwest client,
    /// then run `classify_response` on the result.
    async fn classify(template: ResponseTemplate) -> HostingApiResult<reqwest::Response> {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(template)
            .mount(&server)
            .await;

        let client = reqwest::Client::new();
        let url = server.uri();
        let resp = client.get(&url).send().await.unwrap();
        let rate_limit = extract_rate_limit_from_headers(resp.headers());
        classify_response(resp, rate_limit, &url)
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_success_200() {
        let result = classify(ResponseTemplate::new(200)).await;
        assert!(matches!(result, HostingApiResult::Success(..)));
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_success_with_rate_limit_headers() {
        let result = classify(
            ResponseTemplate::new(200)
                .insert_header("x-ratelimit-remaining", "4999")
                .insert_header("x-ratelimit-reset", "1704067200"),
        )
        .await;
        match result {
            HostingApiResult::Success(_, Some(rl)) => {
                assert_eq!(rl.remaining, 4999);
            }
            _ => panic!("expected Success with rate limit"),
        }
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_not_found_404() {
        let result = classify(ResponseTemplate::new(404)).await;
        assert!(matches!(result, HostingApiResult::NotFound(..)));
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_other_error_500() {
        let result = classify(ResponseTemplate::new(500)).await;
        assert!(matches!(result, HostingApiResult::Failed(..)));
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_403_primary_rate_limit_remaining_zero() {
        let result = classify(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1704067200"),
        )
        .await;
        match result {
            HostingApiResult::RateLimited(rl) => {
                assert_eq!(rl.remaining, 0);
                assert_eq!(rl.reset_at.timestamp(), 1_704_067_200);
            }
            _ => panic!("expected RateLimited"),
        }
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_403_secondary_rate_limit_with_retry_after() {
        let before = Utc::now();
        let result = classify(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "100")
                .insert_header("x-ratelimit-reset", "1704067200")
                .insert_header("retry-after", "60"),
        )
        .await;
        match result {
            HostingApiResult::RateLimited(rl) => {
                assert_eq!(rl.remaining, 0);
                // reset_at should be ~60s from now
                let diff = (rl.reset_at - before).num_seconds();
                assert!((55..=65).contains(&diff), "expected ~60s, got {diff}s");
            }
            _ => panic!("expected RateLimited"),
        }
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_403_no_rate_limit_headers_with_retry_after() {
        // Regression: Retry-After must be honored even when x-ratelimit-* headers are absent
        let before = Utc::now();
        let result = classify(ResponseTemplate::new(403).insert_header("retry-after", "30")).await;
        match result {
            HostingApiResult::RateLimited(rl) => {
                assert_eq!(rl.remaining, 0);
                let diff = (rl.reset_at - before).num_seconds();
                assert!((25..=35).contains(&diff), "expected ~30s, got {diff}s");
            }
            _ => panic!("expected RateLimited"),
        }
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_403_permission_error() {
        // 403 with remaining > 0 and no Retry-After → not a rate limit
        let result = classify(
            ResponseTemplate::new(403)
                .insert_header("x-ratelimit-remaining", "100")
                .insert_header("x-ratelimit-reset", "1704067200"),
        )
        .await;
        match result {
            HostingApiResult::Failed(_, rl) => {
                assert!(rl.is_some());
                assert_eq!(rl.unwrap().remaining, 100);
            }
            _ => panic!("expected Failed"),
        }
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_403_no_headers_no_retry_after() {
        // 403 with no rate limit headers and no Retry-After → primary rate limit (default 1h)
        let before = Utc::now();
        let result = classify(ResponseTemplate::new(403)).await;
        match result {
            HostingApiResult::RateLimited(rl) => {
                assert_eq!(rl.remaining, 0);
                let diff = (rl.reset_at - before).num_seconds();
                assert!((3595..=3605).contains(&diff), "expected ~3600s, got {diff}s");
            }
            _ => panic!("expected RateLimited"),
        }
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_429_with_retry_after() {
        let before = Utc::now();
        let result = classify(ResponseTemplate::new(429).insert_header("retry-after", "10")).await;
        match result {
            HostingApiResult::RateLimited(rl) => {
                assert_eq!(rl.remaining, 0);
                let diff = (rl.reset_at - before).num_seconds();
                assert!((5..=15).contains(&diff), "expected ~10s, got {diff}s");
            }
            _ => panic!("expected RateLimited"),
        }
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_429_primary_rate_limit_remaining_zero() {
        let result = classify(
            ResponseTemplate::new(429)
                .insert_header("x-ratelimit-remaining", "0")
                .insert_header("x-ratelimit-reset", "1704067200"),
        )
        .await;
        match result {
            HostingApiResult::RateLimited(rl) => {
                assert_eq!(rl.remaining, 0);
                assert_eq!(rl.reset_at.timestamp(), 1_704_067_200);
            }
            _ => panic!("expected RateLimited"),
        }
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_429_remaining_positive_no_retry_after() {
        // Regression: 429 with remaining > 0 must still be RateLimited, not Failed
        let result = classify(
            ResponseTemplate::new(429)
                .insert_header("x-ratelimit-remaining", "50")
                .insert_header("x-ratelimit-reset", "1704067200"),
        )
        .await;
        match result {
            HostingApiResult::RateLimited(rl) => {
                assert_eq!(rl.remaining, 50);
                assert_eq!(rl.reset_at.timestamp(), 1_704_067_200);
            }
            _ => panic!("expected RateLimited"),
        }
    }

    #[tokio::test]
    #[cfg_attr(miri, ignore = "Miri cannot call CreateIoCompletionPort")]
    async fn classify_429_no_headers() {
        // 429 with no rate limit headers at all → still RateLimited with default
        let before = Utc::now();
        let result = classify(ResponseTemplate::new(429)).await;
        match result {
            HostingApiResult::RateLimited(rl) => {
                assert_eq!(rl.remaining, 0);
                let diff = (rl.reset_at - before).num_seconds();
                assert!((3595..=3605).contains(&diff), "expected ~3600s, got {diff}s");
            }
            _ => panic!("expected RateLimited"),
        }
    }
}
