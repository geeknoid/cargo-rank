use super::crate_overall_data::CrateOverallData;
use super::crate_version_data::CrateVersionData;
use super::crates_data::CratesData;
use super::owner::Owner;
use super::owner_kind::OwnerKind as PublicOwnerKind;
use super::tables::OwnerKind as TableOwnerKind;
use super::tables::{
    CategoriesTableIndex, CategoryId, CrateId, CratesTableIndex, KeywordId, KeywordsTableIndex, Table, TableMgr, TeamId, TeamsTableIndex,
    UserId, UsersTableIndex, VersionId, VersionsTableIndex,
};
use crate::Result;
use crate::facts::CrateRef;
use crate::facts::ProviderResult;
use crate::facts::crate_spec::CrateSpec;
use crate::facts::progress::Progress;
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use compact_str::{CompactString, ToCompactString};
use core::fmt::Debug;
use core::time::Duration;
use semver::Version as SemverVersion;
use std::collections::BTreeMap;
use crate::{HashMap, HashSet, hash_map_with_capacity, hash_set_with_capacity};
use std::path::Path;
use std::sync::Arc;
use strsim::normalized_damerau_levenshtein;
use url::Url;

const LOG_TARGET: &str = "    crates";
const LOG_TARGET_DB_CONTENT: &str = "db_content";

// Suggestion matching parameters
const MAX_NAME_SUGGESTIONS: usize = 3;
const MIN_SUGGESTION_SCORE: f64 = 0.8;
const MIN_SUGGESTABLE_LEN: usize = 5;

/// Default URL for the crates.io database dump
pub const DEFAULT_DUMP_URL: &str = "https://static.crates.io/db-dump.tar.gz";

#[derive(Debug, Clone)]
pub struct Provider {
    table_mgr: Arc<TableMgr>,
    now: DateTime<Utc>,
}

#[derive(Debug)]
struct PerCrateData {
    crate_index: CratesTableIndex,
    owners: Vec<TableOwnerKind>,
    categories: Vec<CategoryId>,
    keywords: Vec<KeywordId>,
    downloads: u64,
    dependents: u64,
    versions_last_90_days: u64,
    versions_last_180_days: u64,
    versions_last_365_days: u64,
}

// Type aliases for complex return types from phase methods
type VersionScanResult = (
    HashMap<CrateRef, (VersionId, VersionsTableIndex)>,
    HashMap<CrateRef, SemverVersion>,
    HashSet<VersionId>,
    HashMap<VersionId, CrateId>,
    HashMap<VersionId, CrateId>,
);

type LookupTables = (
    HashMap<CategoryId, CategoriesTableIndex>,
    HashMap<KeywordId, KeywordsTableIndex>,
    HashMap<UserId, UsersTableIndex>,
    HashMap<TeamId, TeamsTableIndex>,
);

impl Provider {
    pub async fn new(
        cache_dir: impl AsRef<Path>,
        cache_ttl: Duration,
        progress: Arc<dyn Progress>,
        now: DateTime<Utc>,
        ignore_cached: bool,
        dump_url: Option<&str>,
    ) -> Result<Self> {
        let cache_dir = cache_dir.as_ref().to_path_buf();
        let url = Url::parse(dump_url.unwrap_or(DEFAULT_DUMP_URL))?;
        let table_mgr = TableMgr::new(&url, &cache_dir, cache_ttl, now, ignore_cached, progress).await?;

        Ok(Self {
            table_mgr: Arc::new(table_mgr),
            now,
        })
    }

    /// Get crate data for multiple crates.
    ///
    /// Accepts `CrateRef` which may or may not have a version specified. If no version is specified,
    /// automatically resolves to the latest version during table scanning.
    ///
    /// Returns an iterator of `(CrateSpec, ProviderResult<...>)` pairs where the `CrateSpec` includes
    /// the resolved version. Each result indicates whether the crate was found, not found, or the
    /// version was not found.
    ///
    /// This method orchestrates an 8-phase optimized query pipeline with parallelization:
    /// 1. Build crate name to ID maps and allocate per-crate data structures
    /// 2. Build version requirement maps and track crates needing latest version
    /// 3. Discover dependency relationships for dependent counting
    /// 4. Scan versions table to find requested/latest versions and dependency mappings
    /// 5. Load lookup tables in parallel (categories, keywords, users, teams)
    /// 6. Populate crate data by scanning join tables (owners, categories, keywords)
    /// 7. Collect download statistics in parallel (overall and monthly)
    /// 8. Count dependents and assemble final results
    pub async fn get_crates_data(
        &self,
        crates: &[CrateRef],
        progress: &dyn Progress,
        suggestions: bool,
    ) -> impl Iterator<Item = (CrateSpec, ProviderResult<CratesData>)> {
        let provider = self.clone();

        let message = if crates.len() == 1 {
            format!("Looking for crate '{}'", crates[0].name())
        } else {
            format!("Looking for {} crates", crates.len())
        };
        let message = Arc::new(message);
        progress.set_indeterminate(Box::new(move || (*message).clone()));

        let crates = crates.to_vec();
        tokio::task::spawn_blocking(move || provider.collect_crate_data(crates, suggestions))
            .await
            .expect("tasks must not panic")
            .into_iter()
    }

    fn collect_crate_data(&self, requested: Vec<CrateRef>, suggestions: bool) -> Vec<(CrateSpec, ProviderResult<CratesData>)> {
        let start_time = std::time::Instant::now();
        let requested_names: HashSet<&str> = requested.iter().map(CrateRef::name).collect();

        log::info!(target: LOG_TARGET, "Querying the crates database");

        // Get the timestamp from the database sync
        let timestamp = self.table_mgr.created_at();

        // Phase 1: Build foundational maps from crates table
        let (crate_name_to_id, mut crate_data, suggestions_map) = self.phase1_build_crate_maps(&requested_names, suggestions);

        // Phase 2: Build version requirement maps and track crates needing latest version
        let (needed_versions, need_latest_version) = self.phase2_build_version_requirements(&requested, &crate_name_to_id);

        // Phase 3: Discover dependencies
        let (needed_version_ids, crate_to_dependent_versions) = self.phase3_discover_dependencies(&crate_data);

        // Phase 4: Scan versions table for requested versions, resolve latest versions, and build dependency mappings
        let (version_data_map, resolved_versions, version_ids, version_id_to_crate_id, all_version_to_crate) =
            self.phase4_scan_versions_table(&needed_versions, &need_latest_version, &needed_version_ids, &mut crate_data);

        // Phase 5: Load lookup tables
        let (categories, keywords, users, teams) = self.phase5_load_lookup_tables();

        // Phase 6: Populate crate data from join tables
        self.phase6_populate_join_table_data(&mut crate_data);

        // Phase 7: Collect download statistics
        let (version_monthly_downloads, crate_monthly_downloads) =
            self.phase7_collect_downloads(&mut crate_data, &version_ids, &all_version_to_crate);

        // Phase 8: Count dependents
        count_dependents(&mut crate_data, &crate_to_dependent_versions, &version_id_to_crate_id);

        let results: Vec<_> = requested
            .into_iter()
            .map(|crate_ref| {
                self.assemble_query_result(
                    &crate_ref,
                    timestamp,
                    &crate_name_to_id,
                    &version_data_map,
                    &resolved_versions,
                    &crate_data,
                    &categories,
                    &keywords,
                    &users,
                    &teams,
                    &version_monthly_downloads,
                    &crate_monthly_downloads,
                    &suggestions_map,
                )
            })
            .collect();

        let elapsed = start_time.elapsed();
        log::debug!(
            target: LOG_TARGET,
            "Completed querying the crates database in {:.3}s",
            elapsed.as_secs_f64()
        );

        results
    }

    /// Phase 1: Scan crates table to build name to ID mappings and allocate per-crate data.
    ///
    /// Scans the crates table once, early-exiting when all requested crates are found.
    /// For each requested crate, allocates a `PerCrateData` structure initialized with
    /// empty collections and zero counts.
    ///
    /// If `suggestions` is true, calculates Damerau-Levenshtein similarity scores for missing
    /// crates and tracks up to `MAX_NAME_SUGGESTIONS` suggestions with scores meeting or
    /// exceeding `MIN_SUGGESTION_SCORE`.
    ///
    /// Returns:
    /// - Name to ID mapping for looking up crate IDs by name
    /// - Per-crate data map indexed by `CrateId`
    /// - Suggestions map for crates not found (name to similar crate names, empty if suggestions disabled)
    #[expect(
        clippy::type_complexity,
        reason = "Return type represents three distinct lookup structures needed by caller"
    )]
    fn phase1_build_crate_maps(
        &self,
        requested_names: &HashSet<&str>,
        suggestions: bool,
    ) -> (
        HashMap<CompactString, CrateId>,
        HashMap<CrateId, PerCrateData>,
        HashMap<CompactString, Vec<CompactString>>,
    ) {
        let mut crate_name_to_id = hash_map_with_capacity(requested_names.len());
        let mut crate_data = hash_map_with_capacity(requested_names.len());

        // Pre-compute normalized versions of requested names for efficient similarity matching (only if suggestions enabled)
        let normalized_requested: HashMap<CompactString, CompactString> = if suggestions {
            requested_names
                .iter()
                .filter(|name| name.len() >= MIN_SUGGESTABLE_LEN)
                .map(|&name| (name.to_compact_string(), normalize_name(name)))
                .collect()
        } else {
            HashMap::default()
        };

        let mut suggestions_map: HashMap<CompactString, [Option<(CompactString, f64)>; MAX_NAME_SUGGESTIONS]> = if suggestions {
            normalized_requested
                .keys()
                .map(|s| (s.clone(), [const { None }; MAX_NAME_SUGGESTIONS]))
                .collect()
        } else {
            HashMap::default()
        };

        // Reusable buffer for normalizing crate table names (avoids allocations, only needed if suggestions enabled)
        let mut normalized_buffer = CompactString::new("");

        let mut remaining = requested_names.len();

        if remaining > 0 {
            for (row, index) in self.table_mgr.crates_table().iter() {
                if requested_names.contains(row.name) {
                    // Exact match found
                    let _ = crate_name_to_id.insert(row.name.to_compact_string(), row.id);
                    let _ = crate_data.insert(
                        row.id,
                        PerCrateData {
                            crate_index: index,
                            owners: Vec::new(),
                            categories: Vec::new(),
                            keywords: Vec::new(),
                            downloads: 0,
                            dependents: 0,
                            versions_last_90_days: 0,
                            versions_last_180_days: 0,
                            versions_last_365_days: 0,
                        },
                    );

                    // Remove from suggestions since we found an exact match (only if suggestions enabled)
                    if suggestions {
                        let _ = suggestions_map.remove(row.name);
                    }

                    remaining -= 1;
                    if remaining == 0 {
                        break;
                    }
                } else if suggestions {
                    // Check for similar names using Damerau-Levenshtein on normalized names
                    normalize_name_into(row.name, &mut normalized_buffer);

                    for (requested_name, best) in &mut suggestions_map {
                        let normalized_requested = &normalized_requested[requested_name];
                        let score = normalized_damerau_levenshtein(normalized_requested, &normalized_buffer);
                        if score >= MIN_SUGGESTION_SCORE {
                            // Find the lowest score in the array (or first empty slot)
                            let mut min_idx = 0;
                            let mut min_score = f64::INFINITY;

                            for (idx, slot) in best.iter_mut().enumerate() {
                                match slot {
                                    None => {
                                        // Empty slot found, use it immediately
                                        *slot = Some((CompactString::new(row.name), score));
                                        break;
                                    }
                                    Some((_, s)) if *s < min_score => {
                                        min_score = *s;
                                        min_idx = idx;
                                    }
                                    _ => {}
                                }
                            }

                            // If we found a score to beat (didn't find empty slot), replace it
                            if min_score != f64::INFINITY && score > min_score {
                                best[min_idx] = Some((CompactString::new(row.name), score));
                            }
                        }
                    }
                }
            }
        }

        // Extract just the names, sorted by score (only if suggestions enabled)
        let final_suggestions: HashMap<CompactString, Vec<CompactString>> = if suggestions {
            suggestions_map
                .into_iter()
                .map(|(name, top3)| {
                    let mut suggestions: Vec<_> = top3.into_iter().flatten().collect();

                    // Sort by score descending
                    suggestions.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));

                    // Extract just the names
                    let names = suggestions.into_iter().map(|(name, _)| name).collect();
                    (name, names)
                })
                .collect()
        } else {
            HashMap::default()
        };

        (crate_name_to_id, crate_data, final_suggestions)
    }

    /// Phase 2: Build version requirement maps and track crates needing latest version.
    ///
    /// For `CrateRef` with specified version:
    /// - Creates entry in `CrateId` to (`Version` to `CrateRef`) map
    ///
    /// For `CrateRef` without specified version:
    /// - Adds to `need_latest_version` set: (`CrateId` to `CrateRef`)
    ///
    /// Only builds entries for crates that were found in phase 1.
    #[expect(clippy::unused_self, reason = "Kept as instance method for consistency with other phase methods")]
    fn phase2_build_version_requirements(
        &self,
        requested: &[CrateRef],
        crate_name_to_id: &HashMap<CompactString, CrateId>,
    ) -> (HashMap<CrateId, HashMap<SemverVersion, CrateRef>>, HashMap<CrateId, CrateRef>) {
        let mut needed_versions = hash_map_with_capacity(requested.len());
        let mut need_latest_version = hash_map_with_capacity(requested.len());

        for crate_ref in requested {
            if let Some(&crate_id) = crate_name_to_id.get(crate_ref.name()) {
                if let Some(version) = crate_ref.version() {
                    // Specific version requested
                    let _ = needed_versions
                        .entry(crate_id)
                        .or_insert_with(HashMap::default)
                        .insert(version.clone(), crate_ref.clone());
                } else {
                    // Latest version needed
                    let _ = need_latest_version.insert(crate_id, crate_ref.clone());
                }
            }
        }

        (needed_versions, need_latest_version)
    }

    /// Phase 3: Scan dependencies table to discover which versions depend on our crates.
    ///
    /// Builds two data structures:
    /// 1. Set of all `version_ids` that depend on any of our crates (needed for phase 4)
    /// 2. Mapping of `crate_id` to set of `version_ids` that depend on it (for counting dependents)
    ///
    /// Returns:
    /// - Set of version IDs to look up in versions table
    /// - Map of crate to dependent version IDs for dependent counting
    fn phase3_discover_dependencies(
        &self,
        crate_data: &HashMap<CrateId, PerCrateData>,
    ) -> (HashSet<VersionId>, HashMap<CrateId, HashSet<VersionId>>) {
        let mut needed_version_ids = HashSet::default();
        let mut crate_to_dependent_versions = hash_map_with_capacity(crate_data.len());

        for (row, _) in self.table_mgr.dependencies_table().iter() {
            if crate_data.contains_key(&row.crate_id) {
                let _ = needed_version_ids.insert(row.version_id);
                let _ = crate_to_dependent_versions
                    .entry(row.crate_id)
                    .or_insert_with(HashSet::default)
                    .insert(row.version_id);
            }
        }

        (needed_version_ids, crate_to_dependent_versions)
    }

    /// Phase 4: Scan versions table to find requested versions, resolve latest versions, and build dependency mappings.
    ///
    /// This is a multi-purpose scan that:
    /// 1. Finds the table indices for all requested versions (for data retrieval)
    /// 2. Resolves latest versions for crates where no specific version was requested
    /// 3. Maps `version_ids` back to `crate_ids` (for dependent counting)
    /// 4. Counts versions created in the last 90/180/365 days for each crate
    /// 5. Builds a complete `version_id` to `crate_id` mapping for all versions of our crates (for download aggregation)
    ///
    /// For latest version resolution, tracks the highest version number seen for each crate.
    /// Early-exits once all requested versions, latest versions, and dependency mappings are found
    /// (but only when version-age counting is also complete, which requires a full scan for our crates).
    ///
    /// Returns:
    /// - Map of request index to (`version_id`, table index) for assembling results
    /// - Map of request index to resolved version for crates needing latest
    /// - Set of version IDs for monthly download aggregation
    /// - Map of `version_id` to `crate_id` for dependent counting
    /// - Map of all `version_id` to `crate_id` for our crates (for crate-wide download aggregation)
    fn phase4_scan_versions_table(
        &self,
        needed_versions: &HashMap<CrateId, HashMap<SemverVersion, CrateRef>>,
        need_latest_version: &HashMap<CrateId, CrateRef>,
        needed_version_ids: &HashSet<VersionId>,
        crate_data: &mut HashMap<CrateId, PerCrateData>,
    ) -> VersionScanResult {
        let total_needed_versions: usize = needed_versions.values().map(HashMap::len).sum();

        let mut version_data_map = hash_map_with_capacity(total_needed_versions + need_latest_version.len());
        let mut resolved_versions = hash_map_with_capacity(need_latest_version.len());
        let mut latest_version_indices: HashMap<CrateId, (VersionsTableIndex, SemverVersion)> =
            hash_map_with_capacity(need_latest_version.len());
        let mut version_ids = hash_set_with_capacity(total_needed_versions + need_latest_version.len());
        let mut version_id_to_crate_id = hash_map_with_capacity(needed_version_ids.len());
        let mut all_version_to_crate = HashMap::default();

        let mut remaining_versions = total_needed_versions;
        let remaining_latest = need_latest_version.len();
        let mut remaining_mappings = needed_version_ids.len();

        // Calculate cutoff dates for counting versions in the last 90/180/365 days
        let cutoff_90 = self.now - chrono::Duration::days(90);
        let cutoff_180 = self.now - chrono::Duration::days(180);
        let cutoff_365 = self.now - chrono::Duration::days(365);

        for (lean_row, index) in self.table_mgr.versions_table().iter_lean() {
            // For versions belonging to our crates: do a full row read to access num and created_at.
            // Only ~0.1% of rows match, so the vast majority only pay for the lean read (2 u64s + skips).
            if let Some(data) = crate_data.get_mut(&lean_row.crate_id) {
                let row = self.table_mgr.versions_table().get(index);

                let _ = all_version_to_crate.insert(lean_row.id, lean_row.crate_id);
                if row.created_at >= cutoff_365 {
                    data.versions_last_365_days += 1;
                    if row.created_at >= cutoff_180 {
                        data.versions_last_180_days += 1;
                        if row.created_at >= cutoff_90 {
                            data.versions_last_90_days += 1;
                        }
                    }
                }

                // Check if this is one of our requested versions
                if remaining_versions > 0
                    && let Some(version_map) = needed_versions.get(&lean_row.crate_id)
                    && let Some(crate_ref) = version_map.get(&row.num)
                {
                    let _ = version_data_map.insert(crate_ref.clone(), (lean_row.id, index));
                    let _ = version_ids.insert(lean_row.id);
                    remaining_versions -= 1;
                }

                // Check if this crate needs latest version resolution
                if remaining_latest > 0 && need_latest_version.contains_key(&lean_row.crate_id) {
                    use std::collections::hash_map::Entry;
                    match latest_version_indices.entry(lean_row.crate_id) {
                        Entry::Vacant(e) => {
                            let _ = e.insert((index, row.num.clone()));
                        }
                        Entry::Occupied(mut e) => {
                            let (_, current_best_version) = e.get();
                            if &row.num > current_best_version {
                                *e.get_mut() = (index, row.num.clone());
                            }
                        }
                    }
                }
            }

            // Check if this is a version_id we need for dependency tracking
            if remaining_mappings > 0 && needed_version_ids.contains(&lean_row.id) {
                let _ = version_id_to_crate_id.insert(lean_row.id, lean_row.crate_id);
                remaining_mappings -= 1;
            }

            // Early-exit once we've found everything (can't early-exit for latest versions since we need full scan)
            if remaining_versions == 0 && remaining_mappings == 0 && remaining_latest == 0 {
                break;
            }
        }

        // Convert latest version indices to version_data_map entries
        for (crate_id, (versions_index, version)) in latest_version_indices {
            if let Some(crate_ref) = need_latest_version.get(&crate_id) {
                let version_id = self.table_mgr.versions_table().get(versions_index).id;
                let _ = version_data_map.insert(crate_ref.clone(), (version_id, versions_index));
                let _ = version_ids.insert(version_id);
                let _ = resolved_versions.insert(crate_ref.clone(), version);
            }
        }

        (version_data_map, resolved_versions, version_ids, version_id_to_crate_id, all_version_to_crate)
    }

    /// Phase 5: Load all lookup tables for data enrichment in parallel.
    ///
    /// Loads four lookup tables concurrently using tokio tasks:
    /// - Categories: `category_id` to category name
    /// - Keywords: `keyword_id` to keyword string
    /// - Users: `user_id` to user profile
    /// - Teams: `team_id` to team profile
    ///
    /// These tables are fully loaded into memory for fast random access.
    fn phase5_load_lookup_tables(&self) -> LookupTables {
        let categories = self.load_categories();
        let keywords = self.load_keywords();
        let users = self.load_users();
        let teams = self.load_teams();

        (categories, keywords, users, teams)
    }

    /// Phase 6: Populate crate data by scanning join tables.
    ///
    /// Scans three join tables to populate per-crate collections:
    /// - `crate_owners`: populates owners list
    /// - `crates_categories`: populates categories list
    /// - `crates_keywords`: populates keywords list
    ///
    /// Each table is scanned once in full (no early-exit possible since crates can have
    /// multiple owners/categories/keywords).
    fn phase6_populate_join_table_data(&self, crate_data: &mut HashMap<CrateId, PerCrateData>) {
        self.collect_crate_owners(crate_data);
        self.collect_crate_categories(crate_data);
        self.collect_crate_keywords(crate_data);
    }

    /// Phase 7: Collect download statistics for crates and versions.
    ///
    /// Performs two operations:
    /// 1. Scans `crate_downloads` table to populate overall download counts (with early-exit)
    /// 2. Scans `version_downloads` table once to build monthly download time series for both
    ///    individual versions and whole crates (all versions aggregated)
    ///
    /// Returns tuple of (version-specific monthly downloads, crate-wide monthly downloads).
    #[expect(clippy::type_complexity, reason = "return type clearly represents two related download maps")]
    fn phase7_collect_downloads(
        &self,
        crate_data: &mut HashMap<CrateId, PerCrateData>,
        version_ids: &HashSet<VersionId>,
        all_version_to_crate: &HashMap<VersionId, CrateId>,
    ) -> (HashMap<VersionId, Vec<(NaiveDate, u64)>>, HashMap<CrateId, Vec<(NaiveDate, u64)>>) {
        self.collect_crate_downloads(crate_data);
        self.aggregate_all_monthly_downloads(version_ids, all_version_to_crate, crate_data)
    }

    /// Assemble a single query result from collected data.
    ///
    /// Checks for crate existence and version existence, then assembles the full result
    /// with all enriched data.
    #[expect(clippy::too_many_arguments, reason = "All parameters are distinct lookup maps needed for assembly")]
    fn assemble_query_result(
        &self,
        crate_ref: &CrateRef,
        timestamp: DateTime<Utc>,
        crate_name_to_id: &HashMap<CompactString, CrateId>,
        version_data_map: &HashMap<CrateRef, (VersionId, VersionsTableIndex)>,
        resolved_versions: &HashMap<CrateRef, SemverVersion>,
        crate_data: &HashMap<CrateId, PerCrateData>,
        categories: &HashMap<CategoryId, CategoriesTableIndex>,
        keywords: &HashMap<KeywordId, KeywordsTableIndex>,
        users: &HashMap<UserId, UsersTableIndex>,
        teams: &HashMap<TeamId, TeamsTableIndex>,
        version_monthly_downloads: &HashMap<VersionId, Vec<(NaiveDate, u64)>>,
        crate_monthly_downloads: &HashMap<CrateId, Vec<(NaiveDate, u64)>>,
        suggestions: &HashMap<CompactString, Vec<CompactString>>,
    ) -> (CrateSpec, ProviderResult<CratesData>) {
        // Check if the crate exists
        let Some(&crate_id) = crate_name_to_id.get(crate_ref.name()) else {
            // Build CrateSpec with whatever version we have (or a placeholder)
            let crate_spec = crate_ref
                .to_spec()
                .unwrap_or_else(|| CrateSpec::from_arcs(crate_ref.name_arc(), Arc::new(SemverVersion::new(0, 0, 0))));

            // Get suggestions for this crate name if available
            let similar_names: Arc<[CompactString]> = suggestions
                .get(crate_ref.name())
                .map_or_else(|| Arc::from([]), |v| Arc::from(v.as_slice()));

            return (crate_spec, ProviderResult::CrateNotFound(similar_names));
        };

        // Check if the version was found
        let Some(&(version_id, version_index)) = version_data_map.get(crate_ref) else {
            // Build CrateSpec with the version we were looking for
            let crate_spec = crate_ref
                .to_spec()
                .unwrap_or_else(|| CrateSpec::from_arcs(crate_ref.name_arc(), Arc::new(SemverVersion::new(0, 0, 0))));
            return (crate_spec, ProviderResult::VersionNotFound);
        };

        // Determine the actual version (either specified or resolved)
        // Use Arc to avoid cloning the Version object
        let version_arc = crate_ref
            .version_arc()
            .unwrap_or_else(|| Arc::new(resolved_versions.get(crate_ref).expect("resolved version must exist").clone()));

        // Assemble the full result
        let result = self.assemble_result(
            crate_ref.name(),
            &version_arc,
            timestamp,
            crate_id,
            version_id,
            version_index,
            crate_data,
            categories,
            keywords,
            users,
            teams,
            version_monthly_downloads,
            crate_monthly_downloads,
        );

        // Build the CrateSpec with the resolved version and repository information if available
        // Reuse the Arc pointers from crate_ref and version_arc (no allocations)
        let crate_spec = if let Some(repo_url) = &result.overall_data.repository {
            if let Ok(repo_spec) = crate::facts::repo_spec::RepoSpec::parse(repo_url) {
                CrateSpec::from_arcs_with_repo(crate_ref.name_arc(), version_arc, repo_spec)
            } else {
                log::debug!(target: LOG_TARGET, "Could not parse repository URL for '{}': {}", crate_ref.name(), repo_url);
                CrateSpec::from_arcs(crate_ref.name_arc(), version_arc)
            }
        } else {
            CrateSpec::from_arcs(crate_ref.name_arc(), version_arc)
        };

        (crate_spec, ProviderResult::Found(result))
    }

    fn load_categories(&self) -> HashMap<CategoryId, CategoriesTableIndex> {
        let mut map = hash_map_with_capacity(self.table_mgr.categories_table().len());
        for (row, index) in self.table_mgr.categories_table().iter() {
            let _ = map.insert(row.id, index);
        }
        map
    }

    fn load_keywords(&self) -> HashMap<KeywordId, KeywordsTableIndex> {
        let mut map = hash_map_with_capacity(self.table_mgr.keywords_table().len());
        for (row, index) in self.table_mgr.keywords_table().iter() {
            let _ = map.insert(row.id, index);
        }
        map
    }

    fn load_users(&self) -> HashMap<UserId, UsersTableIndex> {
        let mut map = hash_map_with_capacity(self.table_mgr.users_table().len());
        for (row, index) in self.table_mgr.users_table().iter() {
            let _ = map.insert(row.id, index);
        }
        map
    }

    fn load_teams(&self) -> HashMap<TeamId, TeamsTableIndex> {
        let mut map = hash_map_with_capacity(self.table_mgr.teams_table().len());
        for (row, index) in self.table_mgr.teams_table().iter() {
            let _ = map.insert(row.id, index);
        }
        map
    }

    fn collect_crate_owners(&self, crate_data: &mut HashMap<CrateId, PerCrateData>) {
        for (row, _) in self.table_mgr.crate_owners_table().iter() {
            if let Some(data) = crate_data.get_mut(&row.crate_id) {
                data.owners.push(row.owner());
            }
        }
    }

    fn collect_crate_categories(&self, crate_data: &mut HashMap<CrateId, PerCrateData>) {
        for (row, _) in self.table_mgr.crates_categories_table().iter() {
            if let Some(data) = crate_data.get_mut(&row.crate_id) {
                data.categories.push(row.category_id);
            }
        }
    }

    fn collect_crate_keywords(&self, crate_data: &mut HashMap<CrateId, PerCrateData>) {
        for (row, _) in self.table_mgr.crates_keywords_table().iter() {
            if let Some(data) = crate_data.get_mut(&row.crate_id) {
                data.keywords.push(row.keyword_id);
            }
        }
    }

    fn collect_crate_downloads(&self, crate_data: &mut HashMap<CrateId, PerCrateData>) {
        let mut needed = crate_data.len();
        if needed > 0 {
            for (row, _) in self.table_mgr.crate_downloads_table().iter() {
                if let Some(data) = crate_data.get_mut(&row.crate_id) {
                    data.downloads = row.downloads;

                    needed -= 1;
                    if needed == 0 {
                        break;
                    }
                }
            }
        }
    }

    /// Aggregate monthly downloads for both individual versions and whole crates in a single scan.
    ///
    /// Scans the `version_downloads` table once and simultaneously builds:
    /// 1. Per-version monthly download time series (for the specific queried versions)
    /// 2. Per-crate monthly download time series (across all versions of each crate)
    #[expect(clippy::type_complexity, reason = "return type clearly represents two related download maps")]
    fn aggregate_all_monthly_downloads(
        &self,
        version_ids: &HashSet<VersionId>,
        all_version_to_crate: &HashMap<VersionId, CrateId>,
        crate_data: &HashMap<CrateId, PerCrateData>,
    ) -> (HashMap<VersionId, Vec<(NaiveDate, u64)>>, HashMap<CrateId, Vec<(NaiveDate, u64)>>) {
        let mut version_monthly: HashMap<VersionId, BTreeMap<(i32, u32), u64>> = hash_map_with_capacity(version_ids.len());
        let mut crate_monthly: HashMap<CrateId, BTreeMap<(i32, u32), u64>> = hash_map_with_capacity(crate_data.len());

        for (row, _) in self.table_mgr.version_downloads_table().iter() {
            // Check crate membership first — version_ids is always a subset of all_version_to_crate keys,
            // so a single lookup handles the common rejection path (~99.99% of rows match neither).
            if let Some(&crate_id) = all_version_to_crate.get(&row.version_id) {
                let month_key = (row.date.year(), row.date.month());

                *crate_monthly
                    .entry(crate_id)
                    .or_default()
                    .entry(month_key)
                    .or_insert(0) += row.downloads;

                if version_ids.contains(&row.version_id) {
                    *version_monthly
                        .entry(row.version_id)
                        .or_default()
                        .entry(month_key)
                        .or_insert(0) += row.downloads;
                }
            }
        }

        let version_result = monthly_btree_to_vec(version_monthly);
        let crate_result = monthly_btree_to_vec(crate_monthly);
        (version_result, crate_result)
    }

    #[expect(clippy::too_many_arguments, reason = "Helper method needs access to many data structures")]
    fn assemble_result(
        &self,
        crate_name: &str,
        _version: &SemverVersion,
        _timestamp: DateTime<Utc>,
        crate_id: CrateId,
        version_id: VersionId,
        version_index: VersionsTableIndex,
        crate_data: &HashMap<CrateId, PerCrateData>,
        categories: &HashMap<CategoryId, CategoriesTableIndex>,
        keywords: &HashMap<KeywordId, KeywordsTableIndex>,
        users: &HashMap<UserId, UsersTableIndex>,
        teams: &HashMap<TeamId, TeamsTableIndex>,
        version_monthly_downloads: &HashMap<VersionId, Vec<(NaiveDate, u64)>>,
        crate_monthly_downloads: &HashMap<CrateId, Vec<(NaiveDate, u64)>>,
    ) -> CratesData {
        let version_row = self.table_mgr.versions_table().get(version_index);
        let version_data = CrateVersionData {
            description: version_row.description.into(),
            homepage: version_row.homepage(),
            documentation: version_row.documentation(),
            license: version_row.license.into(),
            rust_version: version_row.rust_version.into(),
            edition: version_row.edition(),
            features: version_row.features(),
            created_at: version_row.created_at,
            updated_at: version_row.updated_at,
            yanked: version_row.yanked,
            downloads: version_row.downloads,
            monthly_downloads: version_monthly_downloads.get(&version_id).cloned().unwrap_or_default(),
        };

        let per_crate_data = crate_data.get(&crate_id).expect("Crate data must exist");

        let crate_row = self.table_mgr.crates_table().get(per_crate_data.crate_index);
        let created_at = crate_row.created_at;
        let updated_at = crate_row.updated_at;
        let repository = crate_row.repository();

        let owners: Vec<Owner> = per_crate_data
            .owners
            .iter()
            .filter_map(|table_owner_kind| match table_owner_kind {
                TableOwnerKind::User(user_id) => {
                    if let Some(&user_index) = users.get(user_id) {
                        let row = self.table_mgr.users_table().get(user_index);
                        Some(Owner {
                            login: row.gh_login.into(),
                            kind: PublicOwnerKind::User,
                            name: (!row.name.is_empty()).then(|| row.name.into()),
                        })
                    } else {
                        log::debug!(target: LOG_TARGET_DB_CONTENT, "User ID {user_id:?} for crate '{crate_name}' not found in users table");
                        None
                    }
                }
                TableOwnerKind::Team(team_id) => {
                    if let Some(&team_index) = teams.get(team_id) {
                        let row = self.table_mgr.teams_table().get(team_index);
                        let name = (!row.name.is_empty()).then(|| row.name.into());
                        Some(Owner {
                            login: row.login.into(),
                            kind: PublicOwnerKind::Team,
                            name,
                        })
                    } else {
                        log::debug!(target: LOG_TARGET_DB_CONTENT, "Team ID {team_id:?} for crate '{crate_name}' not found in teams table");
                        None
                    }
                }
            })
            .collect();

        let category_list: Vec<CompactString> = per_crate_data
            .categories
            .iter()
            .filter_map(|cat_id| {
                if let Some(&cat_index) = categories.get(cat_id) {
                    let row = self.table_mgr.categories_table().get(cat_index);
                    Some(row.slug.into())
                } else {
                    log::debug!(target: LOG_TARGET_DB_CONTENT, "Category ID {cat_id:?} for crate '{crate_name}' not found in categories table");
                    None
                }
            })
            .collect();

        let keyword_list: Vec<CompactString> = per_crate_data
            .keywords
            .iter()
            .filter_map(|kw_id| {
                if let Some(&kw_index) = keywords.get(kw_id) {
                    let row = self.table_mgr.keywords_table().get(kw_index);
                    Some(row.keyword.into())
                } else {
                    log::debug!(target: LOG_TARGET_DB_CONTENT, "Keyword ID {kw_id:?} for crate '{crate_name}' not found in keywords table");
                    None
                }
            })
            .collect();

        CratesData::new(
            version_data,
            CrateOverallData {
                created_at,
                updated_at,
                repository,
                categories: category_list,
                keywords: keyword_list,
                owners,
                monthly_downloads: crate_monthly_downloads.get(&crate_id).cloned().unwrap_or_default(),
                downloads: per_crate_data.downloads,
                dependents: per_crate_data.dependents,
                versions_last_90_days: per_crate_data.versions_last_90_days,
                versions_last_180_days: per_crate_data.versions_last_180_days,
                versions_last_365_days: per_crate_data.versions_last_365_days,
            },
        )
    }
}

/// Convert monthly download maps to sorted vectors.
fn monthly_btree_to_vec<K: Eq + core::hash::Hash>(
    monthly: HashMap<K, BTreeMap<(i32, u32), u64>>,
) -> HashMap<K, Vec<(NaiveDate, u64)>> {
    monthly
        .into_iter()
        .map(|(key, month_map)| {
            let vec = month_map
                .into_iter()
                .filter_map(|((year, month), downloads)| NaiveDate::from_ymd_opt(year, month, 1).map(|date| (date, downloads)))
                .collect();
            (key, vec)
        })
        .collect()
}

/// Normalize a crate name for similarity matching by converting to lowercase and removing separators.
fn normalize_name_into(name: &str, buffer: &mut CompactString) {
    buffer.clear();
    buffer.reserve(name.len());
    for byte in name.bytes() {
        if byte == b'-' || byte == b'_' || byte == b' ' {
            // Skip separator characters
            continue;
        }
        buffer.push(byte.to_ascii_lowercase() as char);
    }
}

/// Create a normalized version of a crate name.
fn normalize_name(name: &str) -> CompactString {
    let mut result = CompactString::with_capacity(name.len());
    normalize_name_into(name, &mut result);
    result
}

fn count_dependents(
    crate_data: &mut HashMap<CrateId, PerCrateData>,
    crate_to_dependent_versions: &HashMap<CrateId, HashSet<VersionId>>,
    version_id_to_crate_id: &HashMap<VersionId, CrateId>,
) {
    // Map version_ids to crate_ids using prebuilt HashMap (no table scan!)
    let mut dependents: HashMap<CrateId, HashSet<CrateId>> = hash_map_with_capacity(crate_data.len());
    for (depended_upon, version_set) in crate_to_dependent_versions {
        for &version_id in version_set {
            if let Some(&crate_id) = version_id_to_crate_id.get(&version_id) {
                let _ = dependents.entry(*depended_upon).or_default().insert(crate_id);
            } else {
                log::debug!(target: LOG_TARGET_DB_CONTENT, "Version ID {version_id:?} references non-existent crate in dependencies calculation for crate '{depended_upon:?}'");
            }
        }
    }

    // Populate crate_data with dependent counts
    for (crate_id, unique_dependents) in dependents {
        if let Some(data) = crate_data.get_mut(&crate_id) {
            data.dependents = unique_dependents.len() as u64;
        }
    }
}

#[cfg(test)]
mod tests {}
