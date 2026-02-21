use super::DocsData;
use crate::Result;
use crate::facts::cache::{Cache, CacheResult};
use crate::facts::crate_spec::CrateSpec;
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::request_tracker::{RequestTracker, TrackedTopic};
use crate::facts::throttler::Throttler;
use crate::facts::ProviderResult;
use futures::stream::TryStreamExt;
use futures_util::future::join_all;
use ohno::{EnrichableExt, IntoAppError, app_err};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

pub(super) const LOG_TARGET: &str = "      docs";

/// Default base URL for docs.rs
pub const DOCS_BASE_URL: &str = "https://docs.rs";

const MAX_CONCURRENT_REQUESTS: usize = 5;

#[derive(Debug, Clone)]
pub struct Provider {
    client: Arc<reqwest::Client>,
    cache: Cache,
    base_url: String,
    throttler: Arc<Throttler>,
}

impl Provider {
    /// Create a new docs provider
    #[must_use]
    pub fn new(cache: Cache, base_url: Option<&str>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("cargo-aprz")
            .build()
            .expect("unable to create HTTP client");

        Self {
            client: Arc::new(client),
            cache,
            base_url: base_url.unwrap_or(DOCS_BASE_URL).to_string(),
            throttler: Throttler::new(MAX_CONCURRENT_REQUESTS),
        }
    }

    /// Get documentation data for multiple crates
    pub async fn get_docs_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
        tracker: &RequestTracker,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<DocsData>)> {
        join_all(crates.into_iter().map(|crate_spec| {
            tracker.add_requests(TrackedTopic::Docs, 1);

            let provider = self.clone();
            let tracker = tracker.clone();

            tokio::spawn(provider.fetch_docs_for_crate(crate_spec, tracker))
        }))
        .await
        .into_iter()
        .map(|task_result| task_result.expect("tasks must not panic"))
        .inspect(|(crate_spec, result)| {
            if let ProviderResult::Error(e) = result {
                log::error!(target: LOG_TARGET, "Could not fetch documentation data for {crate_spec}: {e:#}");
            } else if let ProviderResult::Unavailable(reason) = result {
                log::warn!(target: LOG_TARGET, "Documentation unavailable for {crate_spec}: {reason}");
            }
        })
    }

    async fn fetch_docs_for_crate(self, crate_spec: CrateSpec, tracker: RequestTracker) -> (CrateSpec, ProviderResult<DocsData>) {
        let _permit = self.throttler.acquire().await;
        let result = self.fetch_docs_for_crate_core(&crate_spec).await;
        tracker.complete_request(TrackedTopic::Docs);

        (crate_spec, result)
    }

    async fn fetch_docs_for_crate_core(&self, crate_spec: &CrateSpec) -> ProviderResult<DocsData> {
        let filename = Self::get_cache_filename(crate_spec);

        match self.cache.load::<DocsData>(&filename) {
            CacheResult::Data(data) => return ProviderResult::Found(data),
            CacheResult::NoData(reason) => return ProviderResult::Unavailable(reason.into()),
            CacheResult::Miss => {}
        }

        log::info!(target: LOG_TARGET, "Querying {} for documentation on {crate_spec}", self.base_url);

        let provider = self.clone();
        let spec = crate_spec.clone();

        // resilient_download retries on Err, passes through Ok(None) for 404.
        let result = crate::facts::resilient_http::resilient_download(
            "docs_download",
            spec,
            None,
            move |spec| {
                let provider = provider.clone();
                async move { provider.download_zst_core(&spec).await }
            },
        )
        .await;

        let temp_file = match result {
            Ok(Some(path)) => path,
            Ok(None) => {
                let reason = format!("could not find documentation for {crate_spec} on {}", self.base_url);
                if let Err(e) = self.cache.save_no_data(&filename, &reason) {
                    log::debug!(target: LOG_TARGET, "Could not save cache for {crate_spec}: {e:#}");
                }
                return ProviderResult::Unavailable(reason.into());
            }
            Err(e) => {
                return ProviderResult::Error(Arc::new(e.enrich_with(|| format!("downloading docs for {crate_spec}"))));
            }
        };

        let docs_data = match Self::calculate_docs_metrics(&temp_file, crate_spec) {
            Ok(data) => {
                let m = &data.metrics;
                log::debug!(target: LOG_TARGET, "Successfully calculated docs metrics for {crate_spec}");
                log::debug!(target: LOG_TARGET, "Metrics: coverage={}%, public_api={}, documented={}, examples={}, crate_docs={}",
                    m.doc_coverage_percentage,
                    m.public_api_elements,
                    m.public_api_elements - m.undocumented_elements,
                    m.examples_in_docs,
                    m.has_crate_level_docs);
                data
            }
            Err(e) => {
                let reason = format!("{e:#}");
                if let Err(e) = self.cache.save_no_data(&filename, &reason) {
                    log::debug!(target: LOG_TARGET, "Could not save cache for {crate_spec}: {e:#}");
                }

                tokio::fs::remove_file(&temp_file)
                    .await
                    .unwrap_or_else(|e| log::debug!(target: LOG_TARGET, "Could not remove temp file '{}': {e:#}", temp_file.display()));

                return ProviderResult::Unavailable(reason.into());
            }
        };

        tokio::fs::remove_file(&temp_file)
            .await
            .unwrap_or_else(|e| log::debug!(target: LOG_TARGET, "Could not remove temp file '{}': {e:#}", temp_file.display()));

        match self.cache.save(&filename, &docs_data) {
            Ok(()) => ProviderResult::Found(docs_data),
            Err(e) => ProviderResult::Error(Arc::new(e)),
        }
    }

    /// Get the cache filename for a specific crate and version
    fn get_cache_filename(crate_spec: &CrateSpec) -> String {
        let safe_name = sanitize_path_component(crate_spec.name());
        let safe_version = sanitize_path_component(&crate_spec.version().to_string());
        format!("{safe_name}@{safe_version}.json")
    }

    /// Download logic for a single attempt.
    /// Returns `Ok(None)` for 404 (not retryable), `Ok(Some(path))` on success.
    async fn download_zst_core(&self, crate_spec: &CrateSpec) -> Result<Option<PathBuf>> {
        let crate_name = crate_spec.name();
        let version = crate_spec.version().to_string();

        let url = format!("{}/crate/{crate_name}/{version}/json", self.base_url);

        let response = crate::facts::resilient_http::resilient_get(&self.client, &url)
            .await?;

        let status = response.status();
        if !status.is_success() {
            if status == reqwest::StatusCode::NOT_FOUND {
                return Ok(None);
            }
            let body = response.text().await.unwrap_or_else(|_| String::from("<unable to read body>"));
            log::debug!(target: LOG_TARGET, "Response body (first 500 chars): {}", body.chars().take(500).collect::<String>());
            return Err(app_err!(
                "could not download docs for {crate_spec}: HTTP {status}"
            ));
        }

        // Create a temporary file with sanitized filename
        let temp_dir = std::env::temp_dir();
        let safe_name = sanitize_path_component(crate_name);
        let safe_version = sanitize_path_component(&version);
        let temp_file = temp_dir.join(format!("{safe_name}@{safe_version}.zst"));

        let mut file = tokio::fs::File::create(&temp_file)
            .await
            .into_app_err_with(|| format!("creating temp file '{}'", temp_file.display()))?;

        let mut stream = response.bytes_stream();
        let mut total_bytes = 0;

        while let Some(chunk) = stream
            .try_next()
            .await
            .into_app_err("reading response chunk")?
        {
            total_bytes += chunk.len();
            file.write_all(&chunk)
                .await
                .into_app_err_with(|| format!("writing to temp file '{}'", temp_file.display()))?;
        }

        file.flush()
            .await
            .into_app_err_with(|| format!("flushing temp file '{}'", temp_file.display()))?;

        log::debug!(target: LOG_TARGET, "Downloaded {total_bytes} bytes for {crate_spec} to temp file '{}'", temp_file.display());
        Ok(Some(temp_file))
    }

    fn calculate_docs_metrics(zst_path: impl AsRef<Path>, crate_spec: &CrateSpec) -> Result<DocsData> {
        let path = zst_path.as_ref();
        log::debug!(target: LOG_TARGET, "Opening .zst file for {crate_spec}: {}", path.display());
        let file = fs::File::open(path).into_app_err_with(|| format!("opening file '{}' for {crate_spec}", path.display()))?;

        let decoder = zstd::Decoder::new(file).into_app_err_with(|| format!("creating zstd decoder for {crate_spec}"))?;

        super::calc_metrics::calculate_docs_metrics(decoder, crate_spec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::facts::CrateSpec;
    use semver::Version;

    fn test_crate_spec(name: &str, version: &str) -> CrateSpec {
        CrateSpec::from_arcs(
            Arc::from(name),
            Arc::new(Version::parse(version).unwrap()),
        )
    }

    #[test]
    fn test_get_cache_filename() {
        let spec = test_crate_spec("tokio", "1.2.3");
        let filename = Provider::get_cache_filename(&spec);
        assert_eq!(filename, "tokio@1.2.3.json");
    }

    #[test]
    fn test_get_cache_filename_with_special_chars() {
        let spec = test_crate_spec("my-crate", "0.1.0-beta.1");
        let filename = Provider::get_cache_filename(&spec);
        assert!(filename.contains("my-crate"));
        assert!(Path::new(&filename).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("json")));
    }

    #[test]
    fn test_provider_new_default_url() {
        let cache = Cache::new(
            "/tmp/test",
            core::time::Duration::from_secs(3600),
            chrono::Utc::now(),
            false,
        );
        let provider = Provider::new(cache, None);
        assert_eq!(provider.base_url, DOCS_BASE_URL);
    }

    #[test]
    fn test_provider_new_custom_url() {
        let cache = Cache::new(
            "/tmp/test",
            core::time::Duration::from_secs(3600),
            chrono::Utc::now(),
            false,
        );
        let provider = Provider::new(cache, Some("https://custom.docs.rs"));
        assert_eq!(provider.base_url, "https://custom.docs.rs");
    }
}