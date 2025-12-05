use super::cache_lock::{CacheLockGuard, acquire_cache_lock};
use super::crate_facts::CrateFacts;
use super::crate_spec::CrateSpec;
use super::progress::Progress;
use super::request_tracker::RequestTracker;
use super::{CrateRef, CratesData, ProviderResult};
use crate::Result;
use chrono::{DateTime, Utc};
use core::time::Duration;
use ohno::IntoAppError;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Collector for gathering crate information from different sources
pub struct Collector {
    crates_provider: super::crates::Provider,
    hosting_provider: super::hosting::Provider,
    advisories_provider: super::advisories::Provider,
    codebase_provider: super::codebase::Provider,
    coverage_provider: super::coverage::Provider,
    docs_provider: super::docs::Provider,
    progress: Arc<dyn Progress>,
    _cache_lock: CacheLockGuard,
}

impl core::fmt::Debug for Collector {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Collector")
            .field("crates_provider", &self.crates_provider)
            .field("hosting_provider", &self.hosting_provider)
            .field("advisories_provider", &self.advisories_provider)
            .field("codebase_provider", &self.codebase_provider)
            .field("coverage_provider", &self.coverage_provider)
            .field("docs_provider", &self.docs_provider)
            .field("progress", &"<dyn Progress>")
            .finish_non_exhaustive()
    }
}

impl Collector {
    #[expect(clippy::too_many_arguments, reason = "all cache TTL parameters are necessary for configuration")]
    pub async fn new(
        github_token: Option<&str>,
        codeberg_token: Option<&str>,
        cache_dir: impl AsRef<Path>,
        crates_cache_ttl: Duration,
        hosting_cache_ttl: Duration,
        codebase_cache_ttl: Duration,
        coverage_cache_ttl: Duration,
        advisories_cache_ttl: Duration,
        now: DateTime<Utc>,
        progress: impl Progress + 'static,
    ) -> Result<Self> {
        let progress: Arc<dyn Progress> = Arc::new(progress);
        progress.set_phase("Preparing");

        let crates_cache_dir = create_cache_dir(&cache_dir, "crates")?;
        let hosting_cache_dir = create_cache_dir(&cache_dir, "hosting")?;
        let codebase_cache_dir = create_cache_dir(&cache_dir, "codebase")?;
        let coverage_cache_dir = create_cache_dir(&cache_dir, "coverage")?;
        let advisories_cache_dir = create_cache_dir(&cache_dir, "advisories")?;
        let docs_cache_dir = create_cache_dir(&cache_dir, "docs")?;

        // Acquire cache lock to prevent concurrent access
        let cache_lock = acquire_cache_lock(cache_dir.as_ref()).await?;

        Ok(Self {
            crates_provider: super::crates::Provider::new(&crates_cache_dir, crates_cache_ttl, Arc::clone(&progress), now, None).await?,
            hosting_provider: super::hosting::Provider::new(github_token, codeberg_token, &hosting_cache_dir, hosting_cache_ttl, now)?,
            codebase_provider: super::codebase::Provider::new(&codebase_cache_dir, codebase_cache_ttl, now),
            coverage_provider: super::coverage::Provider::new(&coverage_cache_dir, coverage_cache_ttl, now, None),
            advisories_provider: super::advisories::Provider::new(&advisories_cache_dir, advisories_cache_ttl, Arc::clone(&progress), now)
                .await?,
            docs_provider: super::docs::Provider::new(&docs_cache_dir, now, None),
            progress,
            _cache_lock: cache_lock,
        })
    }

    /// Collect facts for multiple crates
    pub async fn collect(
        &self,
        _now: DateTime<Utc>,
        crate_refs: impl IntoIterator<Item = CrateRef>,
        suggestions: bool,
    ) -> Result<impl Iterator<Item = CrateFacts>> {
        let crate_refs: Vec<_> = crate_refs.into_iter().collect();
        if crate_refs.is_empty() {
            return Ok(Vec::new().into_iter());
        }

        // Step 1: Start identification phase - query crates provider
        self.progress.set_phase("Identifying");
        let crate_data = self
            .crates_provider
            .get_crates_data(crate_refs, self.progress.as_ref(), suggestions)
            .await;

        // Deduplicate CrateSpecs to prevent concurrent processing of the same crate
        let crate_data: Vec<_> = crate_data
            .fold(HashMap::new(), |mut map, (crate_spec, provider_result)| {
                let _ = map.entry(crate_spec).or_insert(provider_result);
                map
            })
            .into_iter()
            .collect();

        // Step 2: Query phase - parallel data gathering
        self.progress.set_phase("Querying");
        let collected_facts = self.query_providers(crate_data).await;

        self.progress.done();

        Ok(collected_facts.into_iter())
    }

    async fn query_providers(&self, crates_data: Vec<(CrateSpec, ProviderResult<CratesData>)>) -> Vec<CrateFacts> {
        let request_tracker = RequestTracker::new(self.progress.as_ref());

        let mut facts_map: HashMap<CrateSpec, CrateFacts> = crates_data
            .into_iter()
            .map(|(crate_spec, crates_result)| {
                let facts = CrateFacts {
                    crate_spec: crate_spec.clone(),
                    crates_data: crates_result,
                    hosting_data: ProviderResult::CrateNotFound(Arc::new([])),
                    advisory_data: ProviderResult::CrateNotFound(Arc::new([])),
                    codebase_data: ProviderResult::CrateNotFound(Arc::new([])),
                    coverage_data: ProviderResult::CrateNotFound(Arc::new([])),
                    docs_data: ProviderResult::CrateNotFound(Arc::new([])),
                };
                (crate_spec, facts)
            })
            .collect();

        let all_queryable_specs: Vec<CrateSpec> = facts_map
            .iter()
            .filter(|(_, facts)| facts.crates_data.is_found())
            .map(|(crate_spec, _)| crate_spec.clone())
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
                    for (crate_spec, result) in $iter {
                        if let Some(facts) = facts_map.get_mut(&crate_spec) {
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
        }

        facts_map.into_values().collect()
    }
}

/// Create a cache directory by joining a base path with a name
fn create_cache_dir(base_path: impl AsRef<Path>, name: impl AsRef<str>) -> Result<PathBuf> {
    let name_str = name.as_ref();
    let cache_path = base_path.as_ref().join(name_str);

    #[cfg(windows)]
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
/// This prevents the approximately 26 second "streaming" lag that occurs when Windows decompresses
/// data on the fly during memory mapping operations.
///
/// This function is completely opportunistic - if it fails for any reason, it fails silently.
#[cfg(windows)]
fn disable_directory_compression(path: impl AsRef<Path>) {
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        COMPRESSION_FORMAT_NONE, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_WRITE_DATA, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;
    use windows::Win32::System::Ioctl::FSCTL_SET_COMPRESSION;
    use windows::core::HSTRING;

    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{addr_of, addr_of_mut};

    /// RAII wrapper for Windows HANDLE that ensures it's closed when dropped
    struct HandleGuard(HANDLE);

    impl Drop for HandleGuard {
        fn drop(&mut self) {
            // SAFETY: handle is valid and we're done using it
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

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

    let _guard = HandleGuard(handle); // Auto-closes handle on drop

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
}
