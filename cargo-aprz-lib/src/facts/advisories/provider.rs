use super::AdvisoryData;
use crate::Result;
use crate::facts::ProviderResult;
use crate::facts::cache_doc;
use crate::facts::crate_spec::CrateSpec;
use crate::facts::progress::Progress;
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
    _timestamp: DateTime<Utc>,
    now: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LastSynced {
    pub timestamp: DateTime<Utc>,
}

const DATABASE_FETCH_TIMEOUT: Duration = Duration::from_secs(60);

impl Provider {
    pub async fn new(cache_dir: impl AsRef<Path>, cache_ttl: Duration, progress: Arc<dyn Progress>, now: DateTime<Utc>) -> Result<Self> {
        let cache_dir = cache_dir.as_ref();
        let sync_path = cache_dir.join("last_synced.json");
        let repo_path = cache_dir.join("repo");

        let timestamp = if let Some(data) =
            cache_doc::load_with_ttl(&sync_path, cache_ttl, |data: &LastSynced| data.timestamp, now, "advisory database")
        {
            data.timestamp
        } else {
            download_db(&repo_path, progress.as_ref())
                .await
                .into_app_err("unable to download the advisory database")?;
            let timestamp = now;
            cache_doc::save(&LastSynced { timestamp }, &sync_path)?;
            timestamp
        };

        Ok(Self {
            database: Arc::new(open_db(&repo_path, progress.as_ref()).await?),
            _timestamp: timestamp,
            now,
        })
    }

    pub async fn get_advisory_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<AdvisoryData>)> {
        let database = Arc::clone(&self.database);
        let now = self.now;

        tokio::task::spawn_blocking(move || scan_advisories(&database, crates, now))
            .await
            .expect("tasks must not panic")
    }
}

fn scan_advisories<I>(
    database: &Database,
    crates: I,
    now: DateTime<Utc>,
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
                timestamp: now,
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

async fn open_db(cache_dir: impl AsRef<Path>, progress: &dyn Progress) -> Result<Database> {
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

async fn download_db(cache_dir: impl AsRef<Path>, progress: &dyn Progress) -> Result<()> {
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

async fn run_blocking_with_progress<T, F>(
    progress: &dyn Progress,
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

    let progress_msg = progress_msg.to_string();
    let start_time = std::time::Instant::now();
    progress.set_indeterminate(Box::new(move || progress_msg.clone()));

    let result = tokio::task::spawn_blocking(blocking_fn).await??;

    let elapsed = start_time.elapsed();
    log::debug!(target: LOG_TARGET, "Finished {success_verb} the advisory database in {:.3}s", elapsed.as_secs_f64());
    Ok(result)
}
