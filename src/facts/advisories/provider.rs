use super::AdvisoryData;
use crate::Result;
use crate::facts::ProviderResult;
use crate::facts::cache_doc;
use crate::facts::crate_spec::CrateSpec;
use crate::facts::progress_reporter::ProgressReporter;
use chrono::{DateTime, Utc};
use core::time::Duration;
use ohno::IntoAppError;
use rustsec::{
    database::Database,
    repository::git::{DEFAULT_URL, Repository},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Log target for advisories provider
const LOG_TARGET: &str = "advisories";

#[derive(Debug)]
pub struct Provider {
    database: Arc<Database>,
    timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LastSynced {
    pub timestamp: DateTime<Utc>,
}

const DATABASE_FETCH_TIMEOUT: Duration = Duration::from_secs(60);

impl Provider {
    pub async fn new(cache_dir: impl AsRef<Path>, cache_ttl: Duration, progress: &ProgressReporter) -> Result<Self> {
        let cache_dir = cache_dir.as_ref();
        let sync_path = cache_dir.join("last_synced.json");
        let repo_path = cache_dir.join("repo");

        let timestamp =
            if let Some(data) = cache_doc::load_with_ttl(&sync_path, cache_ttl, |data: &LastSynced| data.timestamp, "advisory database") {
                data.timestamp
            } else {
                download_db(&repo_path, progress)
                    .await
                    .into_app_err("unable to download the advisory database")?;
                let timestamp = Utc::now();
                cache_doc::save(&LastSynced { timestamp }, &sync_path)?;
                timestamp
            };

        Ok(Self {
            database: Arc::new(open_db(&repo_path, progress).await?),
            timestamp,
        })
    }

    #[must_use]
    pub const fn get_latest_sync(&self) -> DateTime<Utc> {
        self.timestamp
    }

    pub async fn get_advisory_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<AdvisoryData>)> {
        let database = Arc::clone(&self.database);
        let timestamp = self.timestamp;

        tokio::task::spawn_blocking(move || scan_advisories(&database, crates, timestamp))
            .await
            .expect("tasks must not panic")
    }
}

fn scan_advisories<I>(
    database: &Database,
    crates: I,
    timestamp: DateTime<Utc>,
) -> impl Iterator<Item = (CrateSpec, ProviderResult<AdvisoryData>)> + use<I>
where
    I: IntoIterator<Item = CrateSpec>,
{
    let start_time = std::time::Instant::now();

    let mut crate_map: HashMap<String, Vec<(CrateSpec, ProviderResult<AdvisoryData>)>> = HashMap::new();

    for crate_spec in crates {
        crate_map.entry(crate_spec.name().to_owned()).or_default().push((
            crate_spec,
            ProviderResult::Found(AdvisoryData {
                timestamp,
                ..Default::default()
            }),
        ));
    }

    let crate_count = crate_map.len();
    let mut advisories_checked = 0;
    let mut advisories_matched = 0;

    log::info!(target: LOG_TARGET, "Querying the advisory database");

    for advisory in database.iter() {
        advisories_checked += 1;

        if let Some(crate_entries) = crate_map.get_mut(advisory.metadata.package.as_str()) {
            for (crate_spec, result) in crate_entries.iter_mut() {
                advisories_matched += 1;

                if let ProviderResult::Found(data) = result {
                    data.count_advisory_historical(advisory);
                    if advisory.versions.is_vulnerable(crate_spec.version()) {
                        data.count_advisory_for_version(advisory);
                    }
                }
            }
        }
    }

    log::debug!(
        target: LOG_TARGET,
        "Completed scan of advisory database: checked {} advisories, found {} matches for {} crates in {:.3}s",
        advisories_checked,
        advisories_matched,
        crate_count,
        start_time.elapsed().as_secs_f64()
    );

    crate_map.into_values().flatten()
}

async fn open_db(cache_dir: impl AsRef<Path>, progress: &ProgressReporter) -> Result<Database> {
    let cache_path = cache_dir.as_ref().to_path_buf();

    run_blocking_with_progress(
        progress,
        "Opening the advisory database",
        "Opening the advisory database",
        "opening",
        move || Database::open(&cache_path).map_err(Into::into),
    )
    .await
}

async fn download_db(cache_dir: impl AsRef<Path>, progress: &ProgressReporter) -> Result<()> {
    let cache_path = cache_dir.as_ref().to_path_buf();

    run_blocking_with_progress(
        progress,
        &format!("Downloading the advisory database from {DEFAULT_URL}"),
        "Downloading the advisory database",
        "downloading",
        move || {
            Repository::fetch(DEFAULT_URL, &cache_path, true, DATABASE_FETCH_TIMEOUT)
                .map(|_| ())
                .map_err(Into::into)
        },
    )
    .await
}

struct ProgressGuard(tokio::task::JoinHandle<()>);

impl Drop for ProgressGuard {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn run_blocking_with_progress<T, F>(
    progress: &ProgressReporter,
    start_msg: &str,
    progress_msg: &str,
    success_verb: &str,
    blocking_fn: F,
) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    log::info!(target: LOG_TARGET, "{start_msg}");

    progress.enable_indeterminate_mode();
    progress.set_message(format!("0s: {progress_msg}"));

    let progress_clone = progress.clone();
    let progress_msg = progress_msg.to_string();
    let start_time = std::time::Instant::now();
    let _progress_guard = ProgressGuard(tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            let _ = interval.tick().await;
            let elapsed = start_time.elapsed().as_secs();
            progress_clone.set_message(format!("{elapsed}s: {progress_msg}"));
        }
    }));

    let result = tokio::task::spawn_blocking(blocking_fn).await??;

    let elapsed = start_time.elapsed();
    log::debug!(target: LOG_TARGET, "Finished {success_verb} the advisory database in {:.3}s", elapsed.as_secs_f64());
    Ok(result)
}
