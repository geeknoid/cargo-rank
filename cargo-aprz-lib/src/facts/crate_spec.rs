use super::RepoSpec;
use core::cmp::Ordering;
use core::fmt::{Display, Formatter, Result as FmtResult};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CrateSpec {
    name: Arc<str>,
    version: Arc<Version>,
    repo_spec: Option<RepoSpec>,
}

impl CrateSpec {
    #[must_use]
    pub const fn from_arcs(name: Arc<str>, version: Arc<Version>) -> Self {
        Self {
            name,
            version,
            repo_spec: None,
        }
    }

    #[must_use]
    pub const fn from_arcs_with_repo(name: Arc<str>, version: Arc<Version>, repo_spec: RepoSpec) -> Self {
        Self {
            name,
            version,
            repo_spec: Some(repo_spec),
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn version(&self) -> &Version {
        &self.version
    }
}

/// Group crate by their repos
#[must_use]
pub fn by_repo(specs: impl IntoIterator<Item = CrateSpec>) -> HashMap<RepoSpec, Vec<CrateSpec>> {
    let mut repo_crates: HashMap<RepoSpec, Vec<CrateSpec>> = HashMap::new();
    for crate_spec in specs {
        if let Some(repo_spec) = &crate_spec.repo_spec {
            repo_crates.entry(repo_spec.clone()).or_default().push(crate_spec);
        }
    }

    repo_crates
}

impl Display for CrateSpec {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "{}@{}", self.name(), self.version())?;
        Ok(())
    }
}

impl PartialOrd for CrateSpec {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CrateSpec {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name().cmp(other.name()).then_with(|| self.version().cmp(other.version()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use url::Url;

    #[test]
    fn test_from_arcs_without_repo() {
        let name: Arc<str> = Arc::from("tokio");
        let version = Arc::new(Version::parse("1.35.0").unwrap());

        let spec = CrateSpec::from_arcs(name, version);

        assert_eq!(spec.name(), "tokio");
        assert_eq!(spec.version().to_string(), "1.35.0");
    }

    #[test]
    fn test_from_arcs_with_repo() {
        let name: Arc<str> = Arc::from("tokio");
        let version = Arc::new(Version::parse("1.35.0").unwrap());
        let repo_url = Url::parse("https://github.com/tokio-rs/tokio").unwrap();
        let repo_spec = RepoSpec::parse(repo_url).unwrap();

        let spec = CrateSpec::from_arcs_with_repo(name, version, repo_spec);

        assert_eq!(spec.name(), "tokio");
        assert_eq!(spec.version().to_string(), "1.35.0");
    }

    #[test]
    fn test_getters() {
        let name = Arc::from("serde");
        let version = Arc::new(Version::parse("1.0.0").unwrap());
        let spec = CrateSpec::from_arcs(name, version);

        assert_eq!(spec.name(), "serde");
        assert_eq!(spec.version().major, 1);
        assert_eq!(spec.version().minor, 0);
        assert_eq!(spec.version().patch, 0);
    }

    #[test]
    fn test_display_trait() {
        let name = Arc::from("tokio");
        let version = Arc::new(Version::parse("1.35.0").unwrap());
        let spec = CrateSpec::from_arcs(name, version);

        assert_eq!(spec.to_string(), "tokio@1.35.0");
    }

    #[test]
    fn test_display_trait_with_prerelease() {
        let name = Arc::from("alpha-crate");
        let version = Arc::new(Version::parse("0.1.0-alpha.1").unwrap());
        let spec = CrateSpec::from_arcs(name, version);

        assert_eq!(spec.to_string(), "alpha-crate@0.1.0-alpha.1");
    }

    #[test]
    fn test_equality() {
        let name1: Arc<str> = Arc::from("tokio");
        let version1 = Arc::new(Version::parse("1.35.0").unwrap());
        let spec1 = CrateSpec::from_arcs(name1, version1);

        let name2: Arc<str> = Arc::from("tokio");
        let version2 = Arc::new(Version::parse("1.35.0").unwrap());
        let spec2 = CrateSpec::from_arcs(name2, version2);

        assert_eq!(spec1, spec2);
    }

    #[test]
    fn test_inequality_different_name() {
        let spec1 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.35.0").unwrap()));
        let spec2 = CrateSpec::from_arcs(Arc::from("serde"), Arc::new(Version::parse("1.35.0").unwrap()));

        assert_ne!(spec1, spec2);
    }

    #[test]
    fn test_inequality_different_version() {
        let spec1 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.35.0").unwrap()));
        let spec2 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.36.0").unwrap()));

        assert_ne!(spec1, spec2);
    }

    #[test]
    fn test_by_repo_groups_crates_correctly() {
        let tokio_repo = RepoSpec::parse(Url::parse("https://github.com/tokio-rs/tokio").unwrap()).unwrap();
        let serde_repo = RepoSpec::parse(Url::parse("https://github.com/serde-rs/serde").unwrap()).unwrap();

        let tokio_spec =
            CrateSpec::from_arcs_with_repo(Arc::from("tokio"), Arc::new(Version::parse("1.35.0").unwrap()), tokio_repo.clone());

        let tokio_util_spec = CrateSpec::from_arcs_with_repo(
            Arc::from("tokio-util"),
            Arc::new(Version::parse("0.7.10").unwrap()),
            tokio_repo.clone(),
        );

        let serde_spec = CrateSpec::from_arcs_with_repo(Arc::from("serde"), Arc::new(Version::parse("1.0.0").unwrap()), serde_repo.clone());

        let crates = vec![tokio_spec.clone(), tokio_util_spec.clone(), serde_spec.clone()];
        let grouped = by_repo(crates);

        assert_eq!(grouped.len(), 2);
        assert!(grouped.contains_key(&tokio_repo));
        assert!(grouped.contains_key(&serde_repo));

        let tokio_crates = &grouped[&tokio_repo];
        assert_eq!(tokio_crates.len(), 2);
        assert!(tokio_crates.contains(&tokio_spec));
        assert!(tokio_crates.contains(&tokio_util_spec));

        let serde_crates = &grouped[&serde_repo];
        assert_eq!(serde_crates.len(), 1);
        assert!(serde_crates.contains(&serde_spec));
    }

    #[test]
    fn test_by_repo_ignores_crates_without_repo() {
        let tokio_repo = RepoSpec::parse(Url::parse("https://github.com/tokio-rs/tokio").unwrap()).unwrap();

        let with_repo = CrateSpec::from_arcs_with_repo(Arc::from("tokio"), Arc::new(Version::parse("1.35.0").unwrap()), tokio_repo.clone());

        let without_repo = CrateSpec::from_arcs(Arc::from("unknown"), Arc::new(Version::parse("0.1.0").unwrap()));

        let crates = vec![with_repo.clone(), without_repo];
        let grouped = by_repo(crates);

        assert_eq!(grouped.len(), 1);
        assert!(grouped.contains_key(&tokio_repo));

        let tokio_crates = &grouped[&tokio_repo];
        assert_eq!(tokio_crates.len(), 1);
        assert!(tokio_crates.contains(&with_repo));
    }

    #[test]
    fn test_by_repo_empty_input() {
        let crates: Vec<CrateSpec> = vec![];
        let grouped = by_repo(crates);

        assert!(grouped.is_empty());
    }

    #[test]
    fn test_clone_preserves_data() {
        let name = Arc::from("tokio");
        let version = Arc::new(Version::parse("1.35.0").unwrap());
        let repo_url = Url::parse("https://github.com/tokio-rs/tokio").unwrap();
        let repo_spec = RepoSpec::parse(repo_url).unwrap();

        let spec1 = CrateSpec::from_arcs_with_repo(name, version, repo_spec);
        let spec2 = spec1.clone();

        assert_eq!(spec1, spec2);
        assert_eq!(spec1.name(), spec2.name());
        assert_eq!(spec1.version(), spec2.version());
    }

    #[test]
    fn test_hash_consistency() {
        use core::hash::{Hash, Hasher};
        use std::collections::hash_map::DefaultHasher;

        let spec1 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.35.0").unwrap()));
        let spec2 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.35.0").unwrap()));

        let mut hasher1 = DefaultHasher::new();
        spec1.hash(&mut hasher1);
        let hash1 = hasher1.finish();

        let mut hasher2 = DefaultHasher::new();
        spec2.hash(&mut hasher2);
        let hash2 = hasher2.finish();

        // Equal objects must have equal hashes
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_partial_cmp() {
        use core::cmp::Ordering;

        let spec1 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.0.0").unwrap()));
        let spec2 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.1.0").unwrap()));

        assert_eq!(spec1.partial_cmp(&spec2), Some(Ordering::Less));
        assert_eq!(spec2.partial_cmp(&spec1), Some(Ordering::Greater));
        assert_eq!(spec1.partial_cmp(&spec1), Some(Ordering::Equal));
    }

    #[test]
    fn test_cmp_same_name_different_versions() {
        use core::cmp::Ordering;

        let v1 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.0.0").unwrap()));
        let v2 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.0.1").unwrap()));
        let v3 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.1.0").unwrap()));
        let v4 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("2.0.0").unwrap()));

        assert_eq!(v1.cmp(&v2), Ordering::Less);
        assert_eq!(v2.cmp(&v3), Ordering::Less);
        assert_eq!(v3.cmp(&v4), Ordering::Less);
        assert_eq!(v4.cmp(&v1), Ordering::Greater);
    }

    #[test]
    fn test_cmp_different_names_same_version() {
        use core::cmp::Ordering;

        let s1 = CrateSpec::from_arcs(Arc::from("anyhow"), Arc::new(Version::parse("1.0.0").unwrap()));
        let s2 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.0.0").unwrap()));

        assert_eq!(s1.cmp(&s2), Ordering::Less);
        assert_eq!(s2.cmp(&s1), Ordering::Greater);
    }

    #[test]
    fn test_cmp_different_names_and_versions() {
        use core::cmp::Ordering;

        let s1 = CrateSpec::from_arcs(Arc::from("anyhow"), Arc::new(Version::parse("2.0.0").unwrap()));
        let s2 = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.0.0").unwrap()));

        // Name takes precedence over version
        assert_eq!(s1.cmp(&s2), Ordering::Less);
    }

    #[test]
    fn test_sorting() {
        let mut specs = [
            CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("2.0.0").unwrap())),
            CrateSpec::from_arcs(Arc::from("anyhow"), Arc::new(Version::parse("1.0.0").unwrap())),
            CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.0.0").unwrap())),
            CrateSpec::from_arcs(Arc::from("serde"), Arc::new(Version::parse("1.0.0").unwrap())),
        ];

        specs.sort();

        assert_eq!(specs[0].name(), "anyhow");
        assert_eq!(specs[1].name(), "serde");
        assert_eq!(specs[2].name(), "tokio");
        assert_eq!(specs[2].version().to_string(), "1.0.0");
        assert_eq!(specs[3].name(), "tokio");
        assert_eq!(specs[3].version().to_string(), "2.0.0");
    }

    #[test]
    fn test_serialize_without_repo() {
        let spec = CrateSpec::from_arcs(Arc::from("tokio"), Arc::new(Version::parse("1.35.0").unwrap()));
        let json = serde_json::to_string(&spec).unwrap();

        assert!(json.contains("\"name\""));
        assert!(json.contains("\"version\""));
        assert!(json.contains("tokio"));
        assert!(json.contains("1.35.0"));
    }

    #[test]
    fn test_deserialize_without_repo() {
        let json = r#"{"name":"tokio","version":"1.35.0","repo_spec":null}"#;
        let spec: CrateSpec = serde_json::from_str(json).unwrap();

        assert_eq!(spec.name(), "tokio");
        assert_eq!(spec.version().to_string(), "1.35.0");
    }

    #[test]
    fn test_serialize_with_repo() {
        let repo = RepoSpec::parse(Url::parse("https://github.com/tokio-rs/tokio").unwrap()).unwrap();
        let spec = CrateSpec::from_arcs_with_repo(Arc::from("tokio"), Arc::new(Version::parse("1.35.0").unwrap()), repo);

        let json = serde_json::to_string(&spec).unwrap();
        assert!(json.contains("repo_spec"));
    }

    #[test]
    fn test_deserialize_with_repo() {
        let json = r#"{"name":"tokio","version":"1.35.0","repo_spec":{"url":"https://github.com/tokio-rs/tokio","host":"github.com","owner":"tokio-rs","repo":"tokio"}}"#;
        let spec: CrateSpec = serde_json::from_str(json).unwrap();

        assert_eq!(spec.name(), "tokio");
        assert_eq!(spec.version().to_string(), "1.35.0");
    }

    #[test]
    fn test_roundtrip_without_repo() {
        let spec1 = CrateSpec::from_arcs(Arc::from("serde"), Arc::new(Version::parse("1.0.195").unwrap()));
        let json = serde_json::to_string(&spec1).unwrap();
        let spec2: CrateSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(spec1, spec2);
    }

    #[test]
    fn test_roundtrip_with_repo() {
        let repo = RepoSpec::parse(Url::parse("https://github.com/tokio-rs/tokio").unwrap()).unwrap();
        let spec1 = CrateSpec::from_arcs_with_repo(Arc::from("tokio"), Arc::new(Version::parse("1.35.0").unwrap()), repo);

        let json = serde_json::to_string(&spec1).unwrap();
        let spec2: CrateSpec = serde_json::from_str(&json).unwrap();

        assert_eq!(spec1, spec2);
    }

    #[test]
    fn test_display_with_build_metadata() {
        let spec = CrateSpec::from_arcs(Arc::from("mycrate"), Arc::new(Version::parse("1.0.0+build.123").unwrap()));

        assert_eq!(spec.to_string(), "mycrate@1.0.0+build.123");
    }

    #[test]
    fn test_partial_ord_trait() {
        let spec1 = CrateSpec::from_arcs(Arc::from("a"), Arc::new(Version::parse("1.0.0").unwrap()));
        let spec2 = CrateSpec::from_arcs(Arc::from("b"), Arc::new(Version::parse("1.0.0").unwrap()));

        assert!(spec1 < spec2);
        assert!(spec2 > spec1);
        assert!(spec1 <= spec1);
        assert!(spec1 >= spec1);
    }
}
