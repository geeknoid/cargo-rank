use super::{CodebaseData, git, source_file_analyzer};
use crate::Result;
use crate::facts::ProviderResult;
use crate::facts::cache_doc::{CacheEnvelope, EnvelopePayload};
use crate::facts::codebase::github_workflow_analyzer::{GitHubWorkflowInfo, sniff_github_workflows};
use crate::facts::crate_spec::{self, CrateSpec};
use crate::facts::path_utils::sanitize_path_component;
use crate::facts::repo_spec::RepoSpec;
use crate::facts::request_tracker::{RequestTracker, TrackedTopic};
use cargo_metadata::{Metadata, MetadataCommand, PackageId, TargetKind};
use chrono::{DateTime, Utc};
use core::time::Duration;
use futures_util::future::join_all;
use ohno::{EnrichableExt, IntoAppError, app_err};
use crate::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::{JoinHandle, spawn_blocking};

pub(super) const LOG_TARGET: &str = "  codebase";

#[derive(Debug, Clone)]
pub struct Provider {
    cache_dir: Arc<Path>,
    cache_ttl: Duration,
    now: DateTime<Utc>,
    ignore_cached: bool,
}

const METADATA_TIMEOUT: Duration = Duration::from_mins(5);
const GIT_REPO_TIMEOUT: Duration = Duration::from_mins(5);

/// Repository-level data that's shared across all crates in a repository
#[derive(Debug, Clone)]
struct RepoData {
    metadata: Arc<Metadata>,
    workflows: GitHubWorkflowInfo,
    contributor_count: u64,
    commits_last_90_days: u64,
    commits_last_180_days: u64,
    commits_last_365_days: u64,
    commit_count: u64,
    first_commit_at: DateTime<Utc>,
    last_commit_at: DateTime<Utc>,
}

impl Provider {
    #[must_use]
    pub fn new(cache_dir: impl AsRef<Path>, cache_ttl: Duration, now: DateTime<Utc>, ignore_cached: bool) -> Self {
        Self {
            cache_dir: Arc::from(cache_dir.as_ref()),
            cache_ttl,
            now,
            ignore_cached,
        }
    }

    pub async fn get_codebase_data(
        &self,
        crates: impl IntoIterator<Item = CrateSpec> + Send + 'static,
        tracker: &RequestTracker,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<CodebaseData>)> {
        let repo_crates = crate_spec::by_repo(crates);

        tracker.add_requests(TrackedTopic::Codebase, repo_crates.len() as u64);

        // Check cache for all crates from each repo (blocking I/O)
        // If any crate from a repo is expired/missing, we reanalyze all crates from that repo for consistency
        let provider = self.clone();
        let tracker_for_blocking = tracker.clone();

        let (cached_results, needs_repo_fetch) = spawn_blocking(move || {
            let mut cached_results = Vec::new();
            let mut needs_repo_fetch: HashMap<RepoSpec, Vec<CrateSpec>> = HashMap::default();

            for (repo_spec, crates) in repo_crates {
                let mut all_cached_data = Vec::new();
                let mut any_missing = false;

                // Check if all crates from this repo have valid cache
                for crate_spec in &crates {
                    let crate_name = crate_spec.name();
                    let data_path = provider.get_data_path(crate_name, &repo_spec);

                    if provider.ignore_cached {
                        any_missing = true;
                        break;
                    } else if let Some(envelope) = CacheEnvelope::<CodebaseData>::load(
                        &data_path,
                        provider.cache_ttl,
                        provider.now,
                        format!("codebase data for {crate_spec}"),
                    ) {
                        match envelope.payload {
                            EnvelopePayload::Data(cached_data) => {
                                all_cached_data.push((crate_spec.clone(), ProviderResult::Found(cached_data)));
                            }
                            EnvelopePayload::NoData(reason) => {
                                all_cached_data.push((crate_spec.clone(), ProviderResult::Unavailable(reason.into())));
                            }
                        }
                    } else {
                        any_missing = true;
                        break; // No need to check more - we'll reanalyze all
                    }
                }

                if any_missing {
                    // At least one crate is expired/missing, reanalyze all crates from this repo
                    let _ = needs_repo_fetch.insert(repo_spec, crates);
                } else {
                    // All crates have valid cache, use the cached data
                    cached_results.extend(all_cached_data);
                    tracker_for_blocking.complete_request(TrackedTopic::Codebase);
                }
            }

            (cached_results, needs_repo_fetch)
        })
        .await
        .expect("task must not panic");

        // Process each repo end-to-end: fetch repo data, then immediately analyze and cache its crates.
        // This ensures cache files are written as each repo completes, surviving process interruption.
        let repo_results = join_all(needs_repo_fetch.into_iter().map(|(repo_spec, crates)| {
            let provider = self.clone();
            let tracker = tracker.clone();

            tokio::spawn(async move {
                provider.fetch_and_analyze_repo(repo_spec, crates, tracker).await
            })
        }))
        .await
        .into_iter()
        .flat_map(|result| result.expect("task must not panic"));

        // Combine cached and newly analyzed results
        cached_results
            .into_iter()
            .chain(repo_results)
            .inspect(|(crate_spec, result)| {
                if let ProviderResult::Error(e) = result {
                    log::error!(target: LOG_TARGET, "Could not analyze codebase for {crate_spec}: {e:#}");
                } else if let ProviderResult::Unavailable(reason) = result {
                    log::debug!(target: LOG_TARGET, "Codebase data unavailable for {crate_spec}: {reason}");
                }
            })
    }

    /// Fetch repository data and immediately analyze all its crates, writing cache files per-crate.
    async fn fetch_and_analyze_repo(
        self,
        repo_spec: RepoSpec,
        crates: Vec<CrateSpec>,
        tracker: RequestTracker,
    ) -> Vec<(CrateSpec, ProviderResult<CodebaseData>)> {
        // Sync the git repo first — failures here are transient (network) and should not be cached
        let repo_path = self.get_repo_cache_path(&repo_spec);
        if let Err(e) = Self::sync_repo(&repo_path, &repo_spec).await {
            tracker.complete_request(TrackedTopic::Codebase);
            log::error!(target: LOG_TARGET, "Could not sync repository '{repo_spec}': {e:#}");
            let error = Arc::new(e);
            return crates
                .into_iter()
                .map(|crate_spec| (crate_spec, ProviderResult::Error(Arc::clone(&error))))
                .collect();
        }

        let fetch_result = self.fetch_repo_data_core(&repo_spec, &repo_path).await;
        tracker.complete_request(TrackedTopic::Codebase);

        match fetch_result {
            Ok(repo_data) => {
                let repo_data = Arc::new(repo_data);
                join_all(crates.into_iter().map(|crate_spec| {
                    let provider = self.clone();
                    let repo_spec = repo_spec.clone();
                    let repo_data = Arc::clone(&repo_data);

                    tokio::spawn(provider.analyze_crate(crate_spec, repo_spec, repo_data))
                }))
                .await
                .into_iter()
                .map(|result| result.expect("task must not panic"))
                .collect()
            }
            Err(e) => {
                let reason = format!("{e:#}");
                log::error!(target: LOG_TARGET, "Could not analyze repository '{repo_spec}': {reason}");
                crates
                    .into_iter()
                    .map(|crate_spec| {
                        let data_path = self.get_data_path(crate_spec.name(), &repo_spec);
                        if let Err(e) = CacheEnvelope::<CodebaseData>::no_data(self.now, &reason).save(&data_path) {
                            log::debug!(target: LOG_TARGET, "Could not save cache for {crate_spec}: {e:#}");
                        }
                        (crate_spec, ProviderResult::Unavailable(reason.clone().into()))
                    })
                    .collect()
            }
        }
    }

    /// Sync (clone or pull) the git repository. Failures here are transient.
    async fn sync_repo(repo_path: &Path, repo_spec: &RepoSpec) -> Result<()> {
        let git_result = tokio::time::timeout(GIT_REPO_TIMEOUT, git::get_repo(repo_path, repo_spec.url())).await;

        match git_result {
            Err(_) => Err(app_err!(
                "git operation timed out after {} seconds for repository '{repo_spec}'",
                GIT_REPO_TIMEOUT.as_secs(),
            )),
            Ok(Err(e)) => Err(e.enrich_with(|| format!("syncing repository '{repo_spec}'"))),
            Ok(Ok(())) => Ok(()),
        }
    }

    async fn fetch_repo_data_core(&self, repo_spec: &RepoSpec, repo_path: &Path) -> Result<RepoData> {
        let root_manifest = repo_path.join("Cargo.toml");
        if !root_manifest.exists() {
            return Err(app_err!("could not find Cargo.toml in root of repository '{repo_spec}'"));
        }

        log::debug!(target: LOG_TARGET, "Running cargo metadata for repository '{repo_spec}'");
        let timeout_result = tokio::time::timeout(
            METADATA_TIMEOUT,
            spawn_blocking(move || MetadataCommand::new().manifest_path(&root_manifest).exec()),
        )
        .await;

        let metadata = match timeout_result {
            Err(_) => {
                let timeout_secs = METADATA_TIMEOUT.as_secs();
                return Err(app_err!(
                    "cargo metadata timed out after {timeout_secs} seconds for repository '{repo_spec}' - workspace may be too large or Cargo.toml is corrupted"
                ));
            }
            Ok(join_result) => match join_result {
                Ok(Ok(metadata)) => metadata,
                Ok(Err(e)) => {
                    return Err(e).into_app_err_with(|| format!("running cargo metadata for repository '{repo_spec}'"));
                }
                Err(e) => {
                    return Err(e).into_app_err_with(|| format!("joining cargo metadata task for repository '{repo_spec}'"));
                }
            },
        };

        log::debug!(target: LOG_TARGET, "Counting contributors in repository '{repo_spec}'");

        let contributor_count = match git::count_contributors(repo_path).await {
            Ok(count) => count,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "Could not count contributors for '{repo_spec}': {e:#}");
                0
            }
        };

        log::debug!(target: LOG_TARGET, "Counting recent commits in repository '{repo_spec}'");

        let (commits_last_90_days, commits_last_180_days, commits_last_365_days) =
            Self::count_recent_commits(repo_path, repo_spec).await;

        let commit_count = match git::count_all_commits(repo_path).await {
            Ok(count) => count,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "Could not count total commits for '{repo_spec}': {e:#}");
                0
            }
        };

        let first_commit_at = match git::get_first_commit_time(repo_path).await {
            Ok(dt) => dt,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "Could not get first commit time for '{repo_spec}': {e:#}");
                DateTime::UNIX_EPOCH
            }
        };

        let last_commit_at = match git::get_last_commit_time(repo_path).await {
            Ok(dt) => dt,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "Could not get last commit time for '{repo_spec}': {e:#}");
                DateTime::UNIX_EPOCH
            }
        };

        log::debug!(target: LOG_TARGET, "Detecting workflows in repository '{repo_spec}'");

        let repo_path_owned = repo_path.to_path_buf();
        let workflows = match spawn_blocking(move || sniff_github_workflows(&repo_path_owned))
            .await
            .expect("task must not panic")
        {
            Ok(info) => info,
            Err(e) => {
                return Err(e).into_app_err_with(|| format!("analyzing GitHub workflows in repository '{repo_spec}'"));
            }
        };

        log::debug!(target: LOG_TARGET, "Analyzed repository '{repo_spec}', found {} packages", metadata.packages.len());

        Ok(RepoData {
            metadata: Arc::new(metadata),
            workflows,
            contributor_count,
            commits_last_90_days,
            commits_last_180_days,
            commits_last_365_days,
            commit_count,
            first_commit_at,
            last_commit_at,
        })
    }

    async fn count_recent_commits(repo_path: &Path, repo_spec: &RepoSpec) -> (u64, u64, u64) {
        let commits_last_90_days = match git::count_recent_commits(repo_path, 90).await {
            Ok(count) => count,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "Could not count recent commits for '{repo_spec}': {e:#}");
                0
            }
        };

        let commits_last_180_days = match git::count_recent_commits(repo_path, 180).await {
            Ok(count) => count,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "Could not count commits in last 180 days for '{repo_spec}': {e:#}");
                0
            }
        };

        let commits_last_365_days = match git::count_recent_commits(repo_path, 365).await {
            Ok(count) => count,
            Err(e) => {
                log::warn!(target: LOG_TARGET, "Could not count commits in last year for '{repo_spec}': {e:#}");
                0
            }
        };

        (commits_last_90_days, commits_last_180_days, commits_last_365_days)
    }

    /// Analyze a single crate
    async fn analyze_crate(
        self,
        crate_spec: CrateSpec,
        repo_spec: RepoSpec,
        repo_data: Arc<RepoData>,
    ) -> (CrateSpec, ProviderResult<CodebaseData>) {
        let crate_name = crate_spec.name().to_string();
        let data_path = self.get_data_path(&crate_name, &repo_spec);

        log::info!(target: LOG_TARGET, "Analyzing source code for {crate_spec} from repository '{repo_spec}'");

        // Find the package we're interested in
        let Some(package) = repo_data.metadata.packages.iter().find(|p| p.name == crate_name) else {
            let reason = format!("crate '{crate_name}' not found in repository '{repo_spec}'");
            log::debug!(target: LOG_TARGET, "{reason}");
            if let Err(e) = CacheEnvelope::<CodebaseData>::no_data(self.now, &reason).save(&data_path) {
                log::debug!(target: LOG_TARGET, "Could not save cache for {crate_spec}: {e:#}");
            }
            return (crate_spec, ProviderResult::Unavailable(reason.into()));
        };

        let Some(crate_path) = package.manifest_path.parent() else {
            let reason = format!("package manifest has no parent directory for {crate_spec}");
            if let Err(e) = CacheEnvelope::<CodebaseData>::no_data(self.now, &reason).save(&data_path) {
                log::debug!(target: LOG_TARGET, "Could not save cache for {crate_spec}: {e:#}");
            }
            return (crate_spec, ProviderResult::Unavailable(reason.into()));
        };

        log::debug!(target: LOG_TARGET, "Found crate at {crate_path}");

        let example_count = package.targets.iter().filter(|t| t.kind.contains(&TargetKind::Example)).count();
        let transitive_dependencies = Self::count_transitive_dependencies(&package.id, &repo_data.metadata);

        // Create CodebaseData with non-source fields initialized
        let mut codebase_data = CodebaseData {
            source_files_analyzed: 0,
            production_lines: 0,
            test_lines: 0,
            comment_lines: 0,
            unsafe_count: 0,
            source_files_with_errors: 0,
            example_count: example_count as u64,
            transitive_dependencies: transitive_dependencies as u64,
            workflows_detected: repo_data.workflows.workflows_detected,
            miri_detected: repo_data.workflows.miri_detected,
            clippy_detected: repo_data.workflows.clippy_detected,
            contributors: repo_data.contributor_count,
            commits_last_90_days: repo_data.commits_last_90_days,
            commits_last_180_days: repo_data.commits_last_180_days,
            commits_last_365_days: repo_data.commits_last_365_days,
            commit_count: repo_data.commit_count,
            first_commit_at: repo_data.first_commit_at,
            last_commit_at: repo_data.last_commit_at,
        };

        if let Err(e) = Self::analyze_source_files(crate_path.as_std_path(), &mut codebase_data).await {
            let reason = format!("{:#}", e.enrich_with(|| format!("analyzing source files for {crate_spec}")));
            if let Err(e) = CacheEnvelope::<CodebaseData>::no_data(self.now, &reason).save(&data_path) {
                log::debug!(target: LOG_TARGET, "Could not save cache for {crate_spec}: {e:#}");
            }
            return (crate_spec, ProviderResult::Unavailable(reason.into()));
        }

        let now = self.now;
        let result = spawn_blocking({
            move || match CacheEnvelope::data(now, codebase_data.clone()).save(&data_path) {
                Ok(()) => ProviderResult::Found(codebase_data),
                Err(e) => ProviderResult::Error(Arc::new(e)),
            }
        })
        .await
        .expect("task must not panic");

        (crate_spec, result)
    }

    /// Analyze source files in a crate directory
    ///
    /// Walks the `src/` directory and analyzes each Rust file using the source analyzer,
    /// directly updating the provided `CodebaseData` with accumulated statistics.
    /// Uses parallel processing with tokio tasks and a semaphore to limit concurrency.
    async fn analyze_source_files(crate_path: &Path, codebase_data: &mut CodebaseData) -> Result<()> {
        const MAX_FILES: usize = 10_000;
        const MAX_FILE_SIZE: u64 = 5_000_000; // 5MB
        const MAX_DEPTH: usize = 50;

        let src_dir = crate_path.join("src");
        if !src_dir.exists() {
            return Ok(());
        }

        // Collect file paths first (blocking directory walk)
        let src_dir_for_walk = src_dir.clone();
        let file_paths: Vec<_> = spawn_blocking(move || {
            walkdir::WalkDir::new(&src_dir_for_walk)
                .follow_links(false) // Don't follow symlinks to prevent loops
                .max_depth(MAX_DEPTH)
                .into_iter()
                .filter_map(|e| match e {
                    Ok(entry) => Some(entry),
                    Err(err) => {
                        log::debug!(target: LOG_TARGET, "Could not walk directory: {err:#}");
                        None
                    }
                })
                .filter(|e| !e.file_type().is_dir())
                .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("rs"))
                .take(MAX_FILES)
                .filter_map(|entry| {
                    // Check file size before adding to processing queue
                    let metadata = match entry.metadata() {
                        Ok(m) => m,
                        Err(e) => {
                            log::debug!(target: LOG_TARGET, "Could not read metadata for {}: {e:#}", entry.path().display());
                            return None;
                        }
                    };

                    if metadata.len() > MAX_FILE_SIZE {
                        log::debug!(target: LOG_TARGET, "Skipping large file '{}' ({} bytes)", entry.path().display(), metadata.len());
                        return None;
                    }

                    Some(entry.path().to_path_buf())
                })
                .collect()
        })
        .await
        .expect("task must not panic");

        if file_paths.is_empty() {
            return Ok(());
        }

        if file_paths.len() == MAX_FILES {
            log::debug!(
                target: LOG_TARGET,
                "File count limit ({MAX_FILES}) reached in {}, some files may not be analyzed",
                src_dir.display()
            );
        }

        log::debug!(target: LOG_TARGET, "Analyzing {} source files", file_paths.len());

        // Analyze files in parallel with semaphore limiting concurrency
        let num_workers = std::thread::available_parallelism().map(core::num::NonZero::get).unwrap_or(4);
        let semaphore = Arc::new(Semaphore::new(num_workers));
        let mut analysis_tasks: Vec<JoinHandle<Result<_, ohno::AppError>>> = Vec::with_capacity(file_paths.len());
        for path in file_paths {
            let permit_res = Arc::clone(&semaphore).acquire_owned().await;

            let task = spawn_blocking(move || {
                let _permit = permit_res.expect("Semaphore closed");
                let content = fs::read_to_string(&path).into_app_err_with(|| format!("reading source file '{}'", path.display()))?;
                Ok(source_file_analyzer::analyze_source_file(&content))
            });

            analysis_tasks.push(task);
        }

        let results = join_all(analysis_tasks).await;

        for task_result in results {
            match task_result.expect("tasks must not panic") {
                Ok(file_stats) => {
                    codebase_data.source_files_analyzed += 1;
                    codebase_data.production_lines += file_stats.production_lines;
                    codebase_data.test_lines += file_stats.test_lines;
                    codebase_data.comment_lines += file_stats.comment_lines;
                    codebase_data.unsafe_count += file_stats.unsafe_count;

                    if file_stats.has_errors {
                        codebase_data.source_files_with_errors += 1;
                    }
                }
                Err(e) => {
                    log::debug!(target: LOG_TARGET, "Could not read source file, skipping: {e:#}");
                }
            }
        }

        Ok(())
    }

    /// Get the cache path for a specific repository
    fn get_repo_cache_path(&self, repo_spec: &RepoSpec) -> PathBuf {
        let safe_host = sanitize_path_component(repo_spec.host());
        let safe_owner = sanitize_path_component(repo_spec.owner());
        let safe_repo = sanitize_path_component(repo_spec.repo());
        self.cache_dir.join("repos").join(safe_host).join(safe_owner).join(safe_repo)
    }

    /// Get the codebase data file path for a specific crate in a repository
    ///
    /// Returns `cache_dir/analysis/host/owner/repo/crate_name.json`
    fn get_data_path(&self, crate_name: &str, repo_spec: &RepoSpec) -> PathBuf {
        let safe_host = sanitize_path_component(repo_spec.host());
        let safe_owner = sanitize_path_component(repo_spec.owner());
        let safe_repo = sanitize_path_component(repo_spec.repo());
        let safe_crate = sanitize_path_component(crate_name);

        self.cache_dir
            .join("analysis")
            .join(safe_host)
            .join(safe_owner)
            .join(safe_repo)
            .join(format!("{safe_crate}.json"))
    }

    /// Count transitive dependencies by walking the dependency graph
    fn count_transitive_dependencies(package_id: &PackageId, metadata: &Metadata) -> usize {
        use crate::HashSet;
        use std::collections::VecDeque;

        let Some(resolve) = &metadata.resolve else {
            log::debug!(target: LOG_TARGET, "No resolve graph in metadata, cannot count transitive dependencies");
            return 0;
        };

        let node_map: HashMap<&PackageId, &cargo_metadata::Node> = resolve.nodes.iter().map(|n| (&n.id, n)).collect();

        // Find the node for this package in the resolve graph
        let Some(node) = node_map.get(package_id) else {
            log::debug!(target: LOG_TARGET, "Could not find package '{package_id}' in resolve graph, cannot count transitive dependencies");
            return 0;
        };

        // Breadth-first traversal of the dependency graph using references
        let mut visited: HashSet<PackageId> = HashSet::default();
        let mut to_visit: VecDeque<&PackageId> = VecDeque::new();

        // Start with direct dependencies (push references)
        for dep_id in &node.dependencies {
            to_visit.push_back(dep_id);
        }

        // Visit all transitive dependencies
        while let Some(dep_id) = to_visit.pop_front() {
            if !visited.contains(dep_id) {
                let _ = visited.insert(dep_id.clone());

                if let Some(dep_node) = node_map.get(dep_id) {
                    for transitive_dep_id in &dep_node.dependencies {
                        to_visit.push_back(transitive_dep_id);
                    }
                }
            }
        }

        visited.len()
    }
}
