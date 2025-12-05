use super::DocsData;
use crate::Result;
use crate::facts::cache_doc;
use crate::facts::crate_spec::CrateSpec;
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::request_tracker::{RequestTracker, TrackedTopic};
use crate::facts::{DocMetricState, ProviderResult};
use chrono::{DateTime, Utc};
use futures::stream::TryStreamExt;
use futures_util::future::join_all;
use ohno::{EnrichableExt, IntoAppError, app_err};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

pub(super) const LOG_TARGET: &str = "      docs";

/// Default base URL for docs.rs
pub const DEFAULT_DOCS_BASE_URL: &str = "https://docs.rs";

/// Error type for documentation download operations
enum DownloadError {
    NotFound,
    Other(ohno::AppError),
}

#[derive(Debug, Clone)]
pub struct Provider {
    client: Arc<reqwest::Client>,
    cache_dir: Arc<Path>,
    base_url: String,
    now: DateTime<Utc>,
}

impl Provider {
    /// Create a new docs provider
    #[must_use]
    pub fn new(cache_dir: impl AsRef<Path>, now: DateTime<Utc>, base_url: Option<&str>) -> Self {
        let client = reqwest::Client::builder()
            .user_agent("cargo-aprz")
            .build()
            .expect("unable to create HTTP client");

        Self {
            client: Arc::new(client),
            cache_dir: Arc::from(cache_dir.as_ref()),
            base_url: base_url.unwrap_or(DEFAULT_DOCS_BASE_URL).to_string(),
            now,
        }
    }

    /// Get documentation data for multiple crates
    pub async fn get_docs_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
        tracker: &RequestTracker,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<DocsData>)> {
        join_all(crates.into_iter().map(|crate_spec| {
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
            } else if matches!(result, ProviderResult::CrateNotFound(_)) {
                log::warn!(target: LOG_TARGET, "Could not find {crate_spec}");
            } else if let ProviderResult::Found(docs_data) = result
                && let DocMetricState::UnknownFormatVersion(version) = docs_data.metrics {
                    log::warn!(target: LOG_TARGET, "Could not parse documentation data for {crate_spec} due to unsupported rustdoc JSON format version {version}");
            }
        })
    }

    async fn fetch_docs_for_crate(self, crate_spec: CrateSpec, tracker: RequestTracker) -> (CrateSpec, ProviderResult<DocsData>) {
        tracker.add_requests(TrackedTopic::Docs, 1);
        let result = self.fetch_docs_for_crate_core(&crate_spec).await;
        tracker.complete_request(TrackedTopic::Docs);

        (crate_spec, result)
    }

    async fn fetch_docs_for_crate_core(&self, crate_spec: &CrateSpec) -> ProviderResult<DocsData> {
        if let Ok(cached_data) = cache_doc::load::<DocsData>(&self.get_cache_path(crate_spec), format!("docs for crate {crate_spec}")) {
            return ProviderResult::Found(cached_data);
        }

        log::debug!(target: LOG_TARGET, "Cache miss for {crate_spec}, fetching from docs.rs");

        let temp_file = match self.download_zst(crate_spec).await {
            Ok(path) => path,
            Err(DownloadError::NotFound) => {
                return ProviderResult::CrateNotFound(Arc::new([]));
            }
            Err(DownloadError::Other(e)) => {
                return ProviderResult::Error(Arc::new(e.enrich_with(|| format!("could not download docs for {crate_spec}"))));
            }
        };

        let docs_data = match self.calculate_docs_metrics(&temp_file, crate_spec) {
            Ok(data) => {
                match &data.metrics {
                    DocMetricState::Found(metrics) => {
                        log::debug!(target: LOG_TARGET, "Successfully calculated docs metrics for {crate_spec}");
                        log::debug!(target: LOG_TARGET, "Metrics: coverage={}%, public_api={}, documented={}, examples={}, crate_docs={}",
                            metrics.doc_coverage_percentage,
                            metrics.public_api_elements,
                            metrics.public_api_elements - metrics.undocumented_elements,
                            metrics.examples_in_docs,
                            metrics.has_crate_level_docs);
                    }
                    DocMetricState::UnknownFormatVersion(version) => {
                        log::debug!(target: LOG_TARGET, "Unknown rustdoc JSON format version {version} for {crate_spec}");
                    }
                }
                data
            }
            Err(e) => {
                return ProviderResult::Error(Arc::new(
                    e.enrich_with(|| format!("could not calculate documentation metrics for {crate_spec}")),
                ));
            }
        };

        tokio::fs::remove_file(&temp_file)
            .await
            .unwrap_or_else(|e| log::debug!(target: LOG_TARGET, "Could not remove temp file '{}': {e:#}", temp_file.display()));

        let cache_path = self.get_cache_path(crate_spec);
        match cache_doc::save(&docs_data, &cache_path) {
            Ok(()) => ProviderResult::Found(docs_data),
            Err(e) => ProviderResult::Error(Arc::new(e)),
        }
    }

    /// Get the cache file path for a specific crate and version
    fn get_cache_path(&self, crate_spec: &CrateSpec) -> PathBuf {
        let safe_name = sanitize_path_component(crate_spec.name());
        let safe_version = sanitize_path_component(&crate_spec.version().to_string());
        self.cache_dir.join(format!("{safe_name}@{safe_version}.json"))
    }

    /// Download .zst file from docs.rs asynchronously
    async fn download_zst(&self, crate_spec: &CrateSpec) -> Result<PathBuf, DownloadError> {
        let crate_name = crate_spec.name();
        let version = crate_spec.version().to_string();

        let url = format!("{}/crate/{crate_name}/{version}/json", self.base_url);

        log::info!(target: LOG_TARGET, "Querying docs.rs for documentation on {crate_spec}");

        let response = self.client.get(&url).send().await.map_err(|e| DownloadError::Other(e.into()))?;

        let status = response.status();
        if !status.is_success() {
            if status == reqwest::StatusCode::NOT_FOUND {
                return Err(DownloadError::NotFound);
            }
            let body = response.text().await.unwrap_or_else(|_| String::from("<unable to read body>"));
            log::debug!(target: LOG_TARGET, "Response body (first 500 chars): {}", body.chars().take(500).collect::<String>());
            return Err(DownloadError::Other(app_err!(
                "could not download docs for {crate_spec}: HTTP {status}"
            )));
        }

        // Create a temporary file with sanitized filename
        let temp_dir = std::env::temp_dir();
        let safe_name = sanitize_path_component(crate_name);
        let safe_version = sanitize_path_component(&version);
        let temp_file = temp_dir.join(format!("{safe_name}@{safe_version}.zst"));

        let mut file = tokio::fs::File::create(&temp_file)
            .await
            .into_app_err_with(|| format!("could not create temp file '{}'", temp_file.display()))
            .map_err(DownloadError::Other)?;

        let mut stream = response.bytes_stream();
        let mut total_bytes = 0;

        while let Some(chunk) = stream
            .try_next()
            .await
            .into_app_err("could not read response chunk")
            .map_err(DownloadError::Other)?
        {
            total_bytes += chunk.len();
            file.write_all(&chunk)
                .await
                .into_app_err_with(|| format!("could not write to temp file '{}'", temp_file.display()))
                .map_err(DownloadError::Other)?;
        }

        file.flush()
            .await
            .into_app_err_with(|| format!("could not flush temp file '{}'", temp_file.display()))
            .map_err(DownloadError::Other)?;

        log::debug!(target: LOG_TARGET, "Downloaded {total_bytes} bytes for {crate_spec} to temp file '{}'", temp_file.display());
        Ok(temp_file)
    }

    fn calculate_docs_metrics(&self, zst_path: impl AsRef<Path>, crate_spec: &CrateSpec) -> Result<DocsData> {
        let path = zst_path.as_ref();
        log::debug!(target: LOG_TARGET, "Opening .zst file for {crate_spec}: {}", path.display());
        let file = fs::File::open(path).into_app_err_with(|| format!("could not open file '{}' for {crate_spec}", path.display()))?;

        let decoder = zstd::Decoder::new(file).into_app_err_with(|| format!("could not create zstd decoder for {crate_spec}"))?;

        super::calc_metrics::calculate_docs_metrics(decoder, crate_spec, self.now)
    }
}
