use crate::Result;
use crate::facts::cache_doc;
use crate::facts::cache_lock::{CacheLockGuard, acquire_cache_lock};
use crate::facts::crate_facts::CrateFacts;
use crate::facts::crate_spec::CrateSpec;
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::progress_reporter::ProgressReporter;
use crate::facts::request_tracker::RequestTracker;
use crate::facts::{CrateOverallData, CrateRef, CrateVersionData, ProviderResult};
use chrono::{DateTime, Utc};
use core::ptr::{addr_of, addr_of_mut};
use core::time::Duration;
use ohno::IntoAppError;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Log target for collector
const LOG_TARGET: &str = " collector";

/// Result of loading cached facts: (cached facts, missing crate refs)
type LoadCacheResult = (Vec<(CrateSpec, CrateFacts)>, Vec<CrateRef>);

/// Collector for gathering crate information from different sources
#[derive(Debug)]
pub struct Collector {
    crates_provider: crate::facts::crates::Provider,
    hosting_provider: crate::facts::hosting::Provider,
    advisories_provider: crate::facts::advisories::Provider,
    codebase_provider: crate::facts::codebase::Provider,
    coverage_provider: crate::facts::coverage::Provider,
    docs_provider: crate::facts::docs::Provider,
    facts_cache_dir: PathBuf,
    progress: ProgressReporter,
    _cache_lock: CacheLockGuard,
}

impl Collector {
    #[expect(clippy::too_many_arguments, reason = "all cache TTL parameters are necessary for configuration")]
    pub async fn new(
        github_token: Option<&str>,
        cache_dir: impl AsRef<Path>,
        crates_cache_ttl: Duration,
        hosting_cache_ttl: Duration,
        codebase_cache_ttl: Duration,
        coverage_cache_ttl: Duration,
        advisories_cache_ttl: Duration,
        progress: ProgressReporter,
    ) -> Result<Self> {
        progress.set_prefix("Preparing");
        progress.enable_indeterminate_mode();

        let crates_cache_dir = create_cache_dir(&cache_dir, "crates")?;
        let hosting_cache_dir = create_cache_dir(&cache_dir, "hosting")?;
        let codebase_cache_dir = create_cache_dir(&cache_dir, "codebase")?;
        let coverage_cache_dir = create_cache_dir(&cache_dir, "coverage")?;
        let advisories_cache_dir = create_cache_dir(&cache_dir, "advisories")?;
        let docs_cache_dir = create_cache_dir(&cache_dir, "docs")?;
        let facts_cache_dir = create_cache_dir(&cache_dir, "facts")?;

        // Acquire cache lock to prevent concurrent access
        let cache_lock = acquire_cache_lock(cache_dir.as_ref()).await?;

        Ok(Self {
            crates_provider: crate::facts::crates::Provider::new(&crates_cache_dir, crates_cache_ttl, &progress).await?,
            hosting_provider: crate::facts::hosting::Provider::new(github_token, &hosting_cache_dir, hosting_cache_ttl)?,
            codebase_provider: crate::facts::codebase::Provider::new(&codebase_cache_dir, codebase_cache_ttl),
            coverage_provider: crate::facts::coverage::Provider::new(&coverage_cache_dir, coverage_cache_ttl),
            advisories_provider: crate::facts::advisories::Provider::new(&advisories_cache_dir, advisories_cache_ttl, &progress).await?,
            docs_provider: crate::facts::docs::Provider::new(&docs_cache_dir),
            facts_cache_dir,
            progress,
            _cache_lock: cache_lock,
        })
    }

    /// Get a reference to the progress reporter.
    ///
    /// TODO: hack, needs to be removed
    #[must_use]
    pub const fn progress(&self) -> &ProgressReporter {
        &self.progress
    }

    /// Collect facts for multiple crates
    pub async fn collect(&self, crate_refs: impl IntoIterator<Item = CrateRef>) -> Result<impl Iterator<Item = (CrateSpec, CrateFacts)>> {
        let crate_refs: Vec<_> = crate_refs.into_iter().collect();
        if crate_refs.is_empty() {
            return Ok(Vec::new().into_iter());
        }

        let total_crates = crate_refs.len() as u64;

        // Step 1: Get the latest sync times from the db-oriented providers
        let crates_sync_time = self.crates_provider.get_latest_sync();
        let advisory_sync_time = self.advisories_provider.get_latest_sync();
        let min_time = crates_sync_time.max(advisory_sync_time);

        // Step 2: Load cached facts, identify missing crates
        let (mut cached_facts, missing_refs) = self.load_cached_facts(crate_refs, min_time)?;

        // Step 3: If all cached, return early
        if missing_refs.is_empty() {
            self.progress.finish_and_clear();
            return Ok(cached_facts.into_iter());
        }

        // Step 4: Start Analysis phase - query crates provider
        self.progress.set_prefix("Analyzing");
        self.progress.enable_determinate_mode(total_crates);
        self.progress.set_position(0);
        let message = if missing_refs.len() == 1 {
            format!("crate '{}'", missing_refs[0].name())
        } else {
            format!("{} crates", missing_refs.len())
        };
        self.progress.set_message(message);
        self.progress.enable_indeterminate_mode();

        let crate_data = self.crates_provider.get_crate_data(missing_refs).await;

        // Deduplicate CrateSpecs to prevent concurrent writes to cache for the same crate
        let crate_data_vec: Vec<_> = {
            // Collect directly into HashMap for deduplication, keeping first occurrence
            let unique_crates: HashMap<CrateSpec, ProviderResult<(CrateVersionData, CrateOverallData)>> =
                crate_data.fold(HashMap::new(), |mut map, (spec, provider_result)| {
                    let _ = map.entry(spec).or_insert(provider_result);
                    map
                });

            unique_crates.into_iter().collect()
        };

        // Step 5: Collection phase - parallel data gathering (includes advisory queries)
        self.progress.set_prefix("Querying");
        self.progress.enable_determinate_mode(crate_data_vec.len() as u64);
        self.progress.set_position(0);
        self.progress.set_message("");

        let mut collected_facts = self.query_providers(crate_data_vec).await?;

        // Step 7: Merge collected facts with cached facts
        cached_facts.append(&mut collected_facts);

        self.progress.finish_and_clear();

        Ok(cached_facts.into_iter())
    }

    /// Load cached facts for the given crate references
    fn load_cached_facts(&self, crate_refs: impl IntoIterator<Item = CrateRef>, min_time: DateTime<Utc>) -> Result<LoadCacheResult> {
        let mut cached_facts = Vec::new();
        let mut missing_refs = Vec::new();

        for crate_ref in crate_refs {
            let Some(spec) = crate_ref.to_spec() else {
                missing_refs.push(crate_ref);
                continue;
            };

            if let Some(facts) = self.load_from_cache(&spec, min_time)? {
                cached_facts.push((spec, facts));
                continue;
            }

            missing_refs.push(crate_ref);
        }

        Ok((cached_facts, missing_refs))
    }

    /// Try to load crate facts from cache
    fn load_from_cache(&self, crate_spec: &CrateSpec, min_time: DateTime<Utc>) -> Result<Option<CrateFacts>> {
        let cache_path = get_cache_path(&self.facts_cache_dir, crate_spec);

        let facts = match cache_doc::load::<CrateFacts>(&cache_path, format!("facts for {crate_spec}")) {
            Ok(facts) => facts,
            Err(e) => {
                if let Some(io_err) = e.source().and_then(|e| e.downcast_ref::<std::io::Error>())
                    && io_err.kind() == std::io::ErrorKind::NotFound
                {
                    return Ok(None);
                }

                return Err(e).into_app_err_with(|| format!("Could not load '{crate_spec}' data from fact cache"));
            }
        };

        if facts.collected_at >= min_time {
            Ok(Some(facts))
        } else {
            Ok(None)
        }
    }

    async fn query_providers(
        &self,
        crate_datas: Vec<(CrateSpec, ProviderResult<(CrateVersionData, CrateOverallData)>)>,
    ) -> Result<Vec<(CrateSpec, CrateFacts)>> {
        let request_tracker = RequestTracker::new(self.progress.clone());
        let batch_timestamp = Utc::now();

        let mut facts_map: HashMap<CrateSpec, CrateFacts> = crate_datas
            .into_iter()
            .map(|(spec, provider_result)| {
                // Transform ProviderResult<(A, B)> into separate ProviderResult<A> and ProviderResult<B>
                let (version_result, overall_result) = match provider_result {
                    ProviderResult::Found((crate_data, overall_data)) => {
                        (ProviderResult::Found(crate_data), ProviderResult::Found(overall_data))
                    }
                    ProviderResult::CrateNotFound => (ProviderResult::CrateNotFound, ProviderResult::CrateNotFound),
                    ProviderResult::VersionNotFound => (ProviderResult::VersionNotFound, ProviderResult::VersionNotFound),
                    ProviderResult::Error(e) => (ProviderResult::Error(Arc::clone(&e)), ProviderResult::Error(e)),
                };

                let facts = CrateFacts {
                    collected_at: batch_timestamp,
                    crate_version_data: version_result,
                    crate_overall_data: overall_result,
                    hosting_data: ProviderResult::CrateNotFound,
                    advisory_data: ProviderResult::CrateNotFound,
                    codebase_data: ProviderResult::CrateNotFound,
                    coverage_data: ProviderResult::CrateNotFound,
                    docs_data: ProviderResult::CrateNotFound,
                };
                (spec, facts)
            })
            .collect();

        let all_queryable_specs: Vec<CrateSpec> = facts_map
            .iter()
            .filter(|(_, facts)| facts.crate_overall_data.is_found())
            .map(|(spec, _)| spec.clone())
            .collect();

        if !all_queryable_specs.is_empty() {
            let (advisory_iter, docs_iter, hosting_iter, codebase_iter, coverage_iter) = tokio::join!(
                self.advisories_provider.get_advisory_data(all_queryable_specs.clone()),
                self.docs_provider.get_docs_data(all_queryable_specs.clone(), &request_tracker),
                self.hosting_provider
                    .get_hosting_data(all_queryable_specs.clone(), &request_tracker),
                self.codebase_provider
                    .get_codebase_data(all_queryable_specs.clone(), &request_tracker),
                self.coverage_provider.get_coverage_data(all_queryable_specs, &request_tracker),
            );

            macro_rules! update_facts {
                ($iter:expr, $field:ident) => {
                    for (spec, result) in $iter {
                        if let Some(facts) = facts_map.get_mut(&spec) {
                            facts.$field = result;
                        }
                    }
                };
            }

            update_facts!(advisory_iter, advisory_data);
            update_facts!(docs_iter, docs_data);
            update_facts!(hosting_iter, hosting_data);
            update_facts!(codebase_iter, codebase_data);
            update_facts!(coverage_iter, coverage_data);

            for (spec, facts) in &facts_map {
                if facts.is_complete() {
                    log::debug!(target: LOG_TARGET, "Facts are complete for {spec}, saving to cache");
                    let cache_path = get_cache_path(&self.facts_cache_dir, spec);
                    if let Err(e) = cache_doc::save(facts, &cache_path) {
                        log::warn!(target: LOG_TARGET, "Could not cache facts for {spec}: {e}");
                    } else {
                        log::debug!(target: LOG_TARGET, "Successfully cached facts for {spec}");
                    }
                } else {
                    log::debug!(target: LOG_TARGET, "Facts are incomplete for {spec}, not caching. Status: crate_version={}, crate_overall={}, hosting={}, advisory={}, codebase={}, coverage={}, docs={}",
                        facts.crate_version_data.status_str(),
                        facts.crate_overall_data.status_str(),
                        facts.hosting_data.status_str(),
                        facts.advisory_data.status_str(),
                        facts.codebase_data.status_str(),
                        facts.coverage_data.status_str(),
                        facts.docs_data.status_str()
                    );
                }
            }
        }

        Ok(facts_map.into_iter().collect())
    }
}

/// Get the cache file path for a specific crate
fn get_cache_path(facts_cache_dir: impl AsRef<Path>, crate_spec: &CrateSpec) -> PathBuf {
    let safe_name = sanitize_path_component(crate_spec.name());
    let safe_version = sanitize_path_component(&crate_spec.version().to_string());
    facts_cache_dir.as_ref().join(format!("{safe_name}@{safe_version}.json"))
}

/// Create a cache directory by joining a base path with a name
fn create_cache_dir(base_path: impl AsRef<Path>, name: impl AsRef<str>) -> Result<PathBuf> {
    let name_str = name.as_ref();
    let cache_path = base_path.as_ref().join(name_str);
    let needs_creation = !cache_path.exists();

    fs::create_dir_all(&cache_path).into_app_err_with(|| format!("unable to create `{name_str}` cache directory"))?;

    // Disable NTFS compression on crates directory for better memory-mapped file performance
    #[cfg(windows)]
    if needs_creation && name_str == "crates" {
        disable_directory_compression(&cache_path);
    }

    Ok(cache_path)
}

/// Disables NTFS compression on a directory to improve memory-mapped file performance.
///
/// This prevents the ~26 second "streaming" lag that occurs when Windows decompresses
/// data on the fly during memory mapping operations.
///
/// This function is completely opportunistic - if it fails for any reason, it fails silently.
#[cfg(windows)]
fn disable_directory_compression(path: impl AsRef<Path>) {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Storage::FileSystem::{
        COMPRESSION_FORMAT_NONE, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::FSCTL_SET_COMPRESSION;
    use windows::core::HSTRING;

    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    let path = path.as_ref();

    // Convert path to Windows HSTRING via wide string
    let wide_chars: Vec<_> = OsStr::new(path).encode_wide().collect();
    let path_wide = HSTRING::from_wide(&wide_chars);

    // Open the directory with FILE_WRITE_DATA access and FILE_FLAG_BACKUP_SEMANTICS
    // SAFETY: Calling Windows API with valid path
    let handle = unsafe {
        CreateFileW(
            &path_wide,
            FILE_WRITE_DATA.0,                  // Write access needed for DeviceIoControl
            FILE_SHARE_READ | FILE_SHARE_WRITE, // Allow concurrent access
            None,                               // No security attributes
            OPEN_EXISTING,                      // Directory must exist
            FILE_FLAG_BACKUP_SEMANTICS,         // Required to open directories
            None,                               // No template file
        )
    };

    let Ok(handle) = handle else {
        return; // Silently fail if we can't open the directory
    };

    let compression_format = COMPRESSION_FORMAT_NONE;
    let mut bytes_returned: u32 = 0;

    #[expect(clippy::cast_possible_truncation, reason = "size_of::<u16>() is always 2, which fits in u32")]
    // SAFETY: Calling DeviceIoControl with valid handle and compression format
    let _ = unsafe {
        DeviceIoControl(
            handle,
            FSCTL_SET_COMPRESSION,
            Some(addr_of!(compression_format).cast()),
            size_of::<u16>() as u32,
            None,
            0,
            Some(addr_of_mut!(bytes_returned)),
            None,
        )
    };

    // Close the handle explicitly (Windows HANDLEs don't auto-close)
    // SAFETY: handle is valid and we're done using it
    unsafe {
        let _ = CloseHandle(handle);
    }
}
