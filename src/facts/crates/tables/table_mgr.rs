use super::{
    CategoriesTable, CrateDownloadsTable, CrateOwnersTable, CratesCategoriesTable, CratesKeywordsTable, CratesTable, DependenciesTable,
    KeywordsTable, Table, TeamsTable, UsersTable, VersionDownloadsTable, VersionsTable,
};

#[cfg(all_tables)]
use super::{DefaultVersionsTable, MetadataTable, ReservedCrateNamesTable};

use crate::Result;
use crate::facts::progress_reporter::ProgressReporter;
use bytes::Bytes;
use chrono::{DateTime, Utc};
use core::time::Duration;
use flate2::bufread::GzDecoder;
use futures_util::StreamExt;
use ohno::{IntoAppError, app_err, bail};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufRead, Error as IoError, Read};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Instant;
use tar::Archive;
use tokio::sync::mpsc;
use url::Url;

/// Log target for crates tables
const LOG_TARGET: &str = "    crates";

/// Generates the `TableMgr` struct and associated methods from a list of table field definitions.
///
/// Creates:
/// - `TableMgr` struct with fields for each table (wrapped in `Arc`)
/// - Accessor methods for each table (e.g., `crates_table()`, `versions_table()`)
/// - `open_tables_from_scratch()` - Opens all tables from disk
/// - `open_tables_from_files()` - Opens tables from already-open file handles
/// - `delete_all_tables()` - Removes all table files from disk
///
/// Also generates the helper function `process_csv_entry()` used during download.
///
/// See the macro invocation below (lines 189-211) for usage.
macro_rules! define_tables {
    ($(
        $(#[$meta:meta])*
        $field:ident: $type:ty
    ),* $(,)?) => {
        /// Manager for downloading and accessing all crates.io database tables.
        #[derive(Debug)]
        pub struct TableMgr {
            $(
                $(#[$meta])*
                $field: Arc<$type>,
            )*
        }

        impl TableMgr {
            $(
                $(#[$meta])*
                #[must_use]
                pub fn $field(&self) -> &$type {
                    &self.$field
                }
            )*

            fn open_tables_from_scratch(
                tables_root: impl AsRef<Path>,
                max_ttl: Duration,
                progress: &ProgressReporter,
            ) -> Result<Self> {
                const NUM_TABLES: u64 = count_tables!($($field)*);

                progress.enable_determinate_mode(NUM_TABLES);
                progress.set_position(0);
                progress.set_message("Opening tables");

                let mut position = 0u64;
                $(
                    $(#[$meta])*
                    let table_start = Instant::now();
                    $(#[$meta])*
                    log::debug!(target: LOG_TARGET, "Opening table '{}'", <$type>::TABLE_NAME);

                    $(#[$meta])*
                    let table = <$type>::open(&tables_root, max_ttl)
                        .into_app_err(concat!("unable to open ", stringify!($field), " table"))?;
                    $(#[$meta])*
                    let $field = Arc::new(table);

                    $(#[$meta])*
                    {
                        log::debug!(target: LOG_TARGET, "Finished opening table '{}' in {:.3}s", <$type>::TABLE_NAME, table_start.elapsed().as_secs_f64());
                        position += 1;
                        progress.set_position(position);
                    }
                )*

                Ok(Self {
                    $(
                        $(#[$meta])*
                        $field,
                    )*
                })
            }

            fn open_tables_from_files(
                files: HashMap<&'static str, File>,
                max_ttl: Duration,
                progress: &ProgressReporter,
            ) -> Result<Self> {
                const NUM_TABLES: u64 = count_tables!($($field)*);

                progress.enable_determinate_mode(NUM_TABLES);
                progress.set_position(0);
                progress.set_message("Opening tables");

                let mut position = 0u64;
                $(
                    $(#[$meta])*
                    let table_start = Instant::now();
                    $(#[$meta])*
                    log::debug!(target: LOG_TARGET, "Opening table '{}'", <$type>::TABLE_NAME);

                    $(#[$meta])*
                    let file = files.get(<$type>::TABLE_NAME)
                        .into_app_err_with(|| format!("missing file for table {}", <$type>::TABLE_NAME))?;

                    $(#[$meta])*
                    let mmap_start = Instant::now();
                    $(#[$meta])*
                    // SAFETY: We have read-only access to the file for the duration of the mmap
                    let mmap = unsafe {
                        memmap2::Mmap::map(file)
                            .into_app_err_with(|| format!("unable to memory-map {}", <$type>::TABLE_NAME))?
                    };
                    $(#[$meta])*
                    log::debug!(target: LOG_TARGET, "Finished mapping '{}' in {:.3}s", <$type>::TABLE_NAME, mmap_start.elapsed().as_secs_f64());

                    $(#[$meta])*
                    let open_start = Instant::now();
                    $(#[$meta])*
                    let table = <$type>::open_with(mmap, max_ttl)
                        .into_app_err(concat!("unable to open ", stringify!($field), " table"))?;
                    $(#[$meta])*
                    log::debug!(target: LOG_TARGET, "Finished validating {} in {:.3}s", <$type>::TABLE_NAME, open_start.elapsed().as_secs_f64());

                    $(#[$meta])*
                    let $field = Arc::new(table);

                    $(#[$meta])*
                    {
                        log::debug!(target: LOG_TARGET, "Finished opening '{}' in {:.3}s", <$type>::TABLE_NAME, table_start.elapsed().as_secs_f64());
                        position += 1;
                        progress.set_position(position);
                    }
                )*

                Ok(Self {
                    $(
                        $(#[$meta])*
                        $field,
                    )*
                })
            }
        }

        /// Delete all known table files from the tables directory.
        /// Returns false if any file failed to delete due to Windows file locking (error 32).
        /// Returns an error for any other deletion failure.
        fn delete_all_tables(tables_root: impl AsRef<Path>) -> Result<bool> {
            let tables_root = tables_root.as_ref();
            let mut any_locked = false;

            $(
                $(#[$meta])*
                let table_path = tables_root.join(<$type>::TABLE_NAME);
                $(#[$meta])*
                if table_path.exists() {
                    if let Err(e) = fs::remove_file(&table_path) {
                        // Windows error 32 = "The process cannot access the file because it is being used by another process"
                        #[cfg(windows)]
                        if e.raw_os_error() == Some(32) {
                            any_locked = true;
                        } else {
                            return Err(e).into_app_err_with(|| format!("unable to remove {}", table_path.display()));
                        }

                        #[cfg(not(windows))]
                        {
                            return Err(e).into_app_err_with(|| format!("unable to remove {}", table_path.display()));
                        }
                    }
                }
            )*

            Ok(!any_locked)
        }

        fn process_csv_entry(
            filename: &str,
            entry: &mut tar::Entry<impl Read>,
            tables_root: &Path,
        ) -> Result<Option<(&'static str, File)>> {
            match filename {
                $(
                    $(#[$meta])*
                    <$type>::CSV_NAME => {
                        log::info!(target: LOG_TARGET, "Processing CSV file '{}' from database", <$type>::CSV_NAME);
                        let file = <$type>::create_table(tables_root, entry)?;
                        Ok(Some((<$type>::TABLE_NAME, file)))
                    }
                )*
                _ => Ok(None),
            }
        }
    };
}

macro_rules! count_tables {
    () => (0);
    ($head:ident $($tail:ident)*) => (1 + count_tables!($($tail)*));
}

define_tables! {
    crates_table: CratesTable,
    versions_table: VersionsTable,
    version_downloads_table: VersionDownloadsTable,
    dependencies_table: DependenciesTable,
    crate_downloads_table: CrateDownloadsTable,
    crates_categories_table: CratesCategoriesTable,
    crates_keywords_table: CratesKeywordsTable,
    categories_table: CategoriesTable,
    keywords_table: KeywordsTable,
    teams_table: TeamsTable,
    users_table: UsersTable,
    crate_owners_table: CrateOwnersTable,

    #[cfg(all_tables)]
    metadata_table: MetadataTable,

    #[cfg(all_tables)]
    default_versions_table: DefaultVersionsTable,

    #[cfg(all_tables)]
    reserved_crate_names_table: ReservedCrateNamesTable,
}

impl TableMgr {
    pub async fn new(source: &Url, tables_root: impl AsRef<Path>, max_ttl: Duration, progress: &ProgressReporter) -> Result<Self> {
        let tables_root = tables_root.as_ref();

        log::info!("Opening the crates database");
        let result = Self::open_tables_from_scratch(tables_root, max_ttl, progress);

        if let Ok(ref table_mgr) = result {
            log::debug!(
                target: LOG_TARGET,
                "successfully opened cached crates.io tables from {} (created at {})",
                tables_root.display(),
                table_mgr.created_at()
            );
            return result;
        }

        log::info!(target: LOG_TARGET, "Cached crates.io database not found or out of date, downloading a fresh copy");

        if let Err(e) = Self::cleanup_tables(tables_root) {
            log::debug!(
                target: LOG_TARGET,
                "unable to cleanup stale table files from {}, continuing anyway: {}",
                tables_root.display(),
                e
            );
        }

        match prep_tables(source, tables_root, max_ttl, progress).await {
            Ok(table_mgr) => Ok(table_mgr),
            Err(e) => Err(e).into_app_err_with(|| "could not prepare crates.io tables"),
        }
    }

    #[must_use]
    pub fn created_at(&self) -> DateTime<Utc> {
        self.crates_table.timestamp()
    }

    fn cleanup_tables(tables_root: impl AsRef<Path>) -> Result<()> {
        const MAX_WAIT_MS: u64 = 4000;
        const INITIAL_DELAY_MS: u64 = 100;
        const MAX_DELAY_MS: u64 = 1000;

        let tables_root = tables_root.as_ref();

        // On Windows, memory-mapped files might not be immediately released after dropping.
        // This is a documented OS limitation where kernel cleanup is asynchronous.
        // Retry with exponential backoff up to 4 seconds total wait time.

        let start = Instant::now();
        let mut delay_ms = INITIAL_DELAY_MS;

        loop {
            if delete_all_tables(tables_root)? {
                return Ok(());
            }

            #[expect(
                clippy::cast_possible_truncation,
                reason = "Elapsed time won't exceed u64::MAX in practice (would require ~584 million years)"
            )]
            let elapsed_ms = start.elapsed().as_millis() as u64;

            // If we've already waited MAX_WAIT_MS, give up
            if elapsed_ms >= MAX_WAIT_MS {
                return Err(app_err!(
                    "unable to remove all table files in {}: some files remain locked after {}ms of retrying",
                    tables_root.display(),
                    elapsed_ms,
                ));
            }

            // Calculate how long to sleep (don't exceed MAX_WAIT_MS total)
            let remaining_ms = MAX_WAIT_MS - elapsed_ms;
            let sleep_ms = delay_ms.min(remaining_ms);

            #[expect(
                clippy::cast_precision_loss,
                reason = "sleep_ms is capped at 1000ms, well within f64 precision range"
            )]
            let sleep_seconds = sleep_ms as f64 / 1000.0;

            log::debug!(
                target: LOG_TARGET,
                "unable to delete all table files in {}, retrying in {} seconds",
                tables_root.display(),
                sleep_seconds
            );

            thread::sleep(Duration::from_millis(sleep_ms));

            // Exponential backoff for next iteration, capped at MAX_DELAY_MS
            delay_ms = (delay_ms * 2).min(MAX_DELAY_MS);
        }
    }
}

// As we get data off the socket, we transfer the chunks over to the thread responsible for decompression and saving to disk.
// There can be up to NUM_CHANNEL_BUFFERS chunks "in flight" at any given time. If we can't keep up writing to disk,
// the channel will fill up, which will eventually cause the network to stop pumping data until there is space in the channel.
const NUM_CHANNEL_BUFFERS: usize = 64;

async fn prep_tables(source: &Url, tables_root: impl AsRef<Path>, max_ttl: Duration, progress: &ProgressReporter) -> Result<TableMgr> {
    log::info!(target: LOG_TARGET, "Starting crates.io database download from {source}");

    let client = reqwest::Client::builder()
        .user_agent("cargo-rank")
        .build()
        .into_app_err("unable to create HTTP client")?;
    let response = client
        .get(source.clone())
        .send()
        .await
        .into_app_err("unable to start downloading crates.io database dump")?;

    if !response.status().is_success() {
        bail!("unable to download crates.io database dump: HTTP {}", response.status());
    }

    let content_length = response.content_length();

    if content_length.is_some() {
        progress.enable_determinate_mode(content_length.unwrap_or(0));
    } else {
        progress.enable_indeterminate_mode();
    }

    let (tx, rx) = mpsc::channel::<Result<Bytes>>(NUM_CHANNEL_BUFFERS);
    let download_progress = progress.clone();
    let processing_progress = progress.clone();
    let tables_root = tables_root.as_ref().to_path_buf();
    let processing_handle = tokio::task::spawn_blocking(move || process_download(rx, &tables_root, max_ttl, &processing_progress));
    let mut stream = response.bytes_stream();
    let mut downloaded = 0u64;

    while let Some(chunk) = stream.next().await {
        match chunk {
            Ok(bytes) => {
                downloaded += bytes.len() as u64;

                download_progress.set_position(downloaded);
                let mb_downloaded = downloaded / 1_000_000;
                if let Some(total) = content_length {
                    let mb_total = total / 1_000_000;
                    download_progress.set_message(format!("{mb_downloaded}/{mb_total} MB: Downloading crates.io database"));
                } else {
                    download_progress.set_message(format!("{mb_downloaded} MB: Downloading crates.io database"));
                }

                if tx.send(Ok(bytes)).await.is_err() {
                    break;
                }
            }
            Err(e) => {
                let _ = tx.send(Err(ohno::app_err!("download failed: {e}"))).await;
                break;
            }
        }
    }

    if let Some(total) = content_length {
        download_progress.set_position(total);
    }

    drop(tx);
    let table_mgr = processing_handle.await??;

    Ok(table_mgr)
}

fn process_download(
    rx: mpsc::Receiver<Result<Bytes>>,
    tables_root: &Path,
    max_ttl: Duration,
    progress: &ProgressReporter,
) -> Result<TableMgr> {
    log::info!(target: LOG_TARGET, "Processing crates.io database download");
    let reader = ChannelReader::new(rx);
    let decoder = GzDecoder::new(reader);
    let mut archive = Archive::new(decoder);

    let mut files = HashMap::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let start = Instant::now();
        if let Some((table_name, file)) = process_csv_entry(filename, &mut entry, tables_root)? {
            let _ = files.insert(table_name, file);
            log::info!(
                target: LOG_TARGET,
                "Finished processing CSV file '{}' in {:.3}s",
                filename,
                start.elapsed().as_secs_f64()
            );
        }
    }

    let table_mgr = TableMgr::open_tables_from_files(files, max_ttl, progress)?;

    Ok(table_mgr)
}

struct ChannelReader {
    rx: mpsc::Receiver<Result<Bytes>>,
    current_chunk: Option<Bytes>,
    position: usize,
}

impl ChannelReader {
    const fn new(rx: mpsc::Receiver<Result<Bytes>>) -> Self {
        Self {
            rx,
            current_chunk: None,
            position: 0,
        }
    }
}

impl BufRead for ChannelReader {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        while self.current_chunk.as_ref().is_none_or(|chunk| self.position >= chunk.len()) {
            match self.rx.blocking_recv() {
                Some(Ok(chunk)) => {
                    self.current_chunk = Some(chunk);
                    self.position = 0;
                }
                Some(Err(e)) => return Err(IoError::other(e.into_std_error())),
                None => return Ok(&[]),
            }
        }

        Ok(&self.current_chunk.as_ref().expect("guaranteed by while condition")[self.position..])
    }

    fn consume(&mut self, amount: usize) {
        self.position += amount;
    }
}

impl Read for ChannelReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let data = self.fill_buf()?;
        let to_copy = data.len().min(buf.len());
        buf[..to_copy].copy_from_slice(&data[..to_copy]);
        self.consume(to_copy);
        Ok(to_copy)
    }
}

#[cfg(test)]
mod tests {
    use crate::facts::crates::tables::{Table, TableMgr};
    use crate::facts::progress_reporter::ProgressReporter;
    use core::time::Duration;
    use std::env;
    use url::Url;

    #[tokio::test]
    #[ignore = "This test downloads real data, run explicitly with --ignored"]
    async fn test_iterate_all_tables() {
        // Get the crates.io database dump URL
        let url = Url::parse("https://static.crates.io/db-dump.tar.gz").expect("Failed to parse URL");

        // Use a temporary directory for tables
        let temp_dir = env::temp_dir().join("cargo-rank-test-tables");
        std::fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        println!("Downloading and loading tables to {temp_dir:?}");

        // Load the table manager (this will download if needed)
        let progress = ProgressReporter::new(Duration::MAX);
        let cache_ttl = Duration::from_secs(365 * 24 * 60 * 60); // 365 days
        let table_mgr = TableMgr::new(&url, &temp_dir, cache_ttl, &progress)
            .await
            .expect("Failed to load table manager");

        println!("Tables loaded successfully\n");

        // Test iterating through each table
        println!("Testing crates_table...");
        let mut count = 0;
        for (_row, _) in table_mgr.crates_table().iter() {
            count += 1;
        }
        println!("  Read {count} rows from crates_table\n");

        println!("Testing versions_table...");
        count = 0;
        for (_row, _) in table_mgr.versions_table().iter() {
            count += 1;
        }
        println!("  Read {count} rows from versions_table\n");

        println!("Testing dependencies_table...");
        count = 0;
        for (_row, _) in table_mgr.dependencies_table().iter() {
            count += 1;
        }
        println!("  Read {count} rows from dependencies_table\n");

        println!("Testing crate_owners_table...");
        count = 0;
        for (_row, _) in table_mgr.crate_owners_table().iter() {
            count += 1;
        }
        println!("  Read {count} rows from crate_owners_table\n");

        println!("Testing users_table...");
        count = 0;
        for (_row, _) in table_mgr.users_table().iter() {
            count += 1;
        }
        println!("  Read {count} rows from users_table\n");

        println!("Testing teams_table...");
        count = 0;
        for (_row, _) in table_mgr.teams_table().iter() {
            count += 1;
        }
        println!("  Read {count} rows from teams_table\n");

        println!("Testing categories_table...");
        count = 0;
        for (_row, _) in table_mgr.categories_table().iter() {
            count += 1;
        }
        println!("  Read {count} rows from categories_table\n");

        println!("Testing keywords_table...");
        count = 0;
        for (_row, _) in table_mgr.keywords_table().iter() {
            count += 1;
        }
        println!("  Read {count} rows from keywords_table\n");

        println!("All table iterations completed successfully!");
    }
}
