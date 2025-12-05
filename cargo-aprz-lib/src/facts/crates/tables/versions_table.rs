use super::super::rust_edition::RustEdition;
#[cfg(all_fields)]
use super::UserId;
use super::{CrateId, VersionId, define_rows, define_table};
use crate::Result;
use chrono::{DateTime, Utc};
use ohno::IntoAppError;
use semver::Version;
use std::collections::BTreeMap;
use url::Url;

/// Log target for versions table
const LOG_TARGET: &str = "    crates";

define_rows! {
    VersionRow<'a> {
        pub id: VersionId,
        pub crate_id: CrateId,
        pub num: Version,
        pub downloads: u64,
        edition: Option<u64>,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
        pub description: &'a str,
        features: &'a str,
        pub license: &'a str,
        pub rust_version: &'a str,
        pub yanked: bool,
        documentation: &'a str,
        homepage: &'a str,
        #[cfg(all_fields)]
        categories: Vec<&'a str>,
        #[cfg(all_fields)]
        keywords: Vec<&'a str>,
        #[cfg(all_fields)]
        repository: &'a str,
        #[cfg(all_fields)]
        pub links: &'a str,
        #[cfg(all_fields)]
        pub bin_names: &'a str,
        #[cfg(all_fields)]
        pub checksum: &'a str,
        #[cfg(all_fields)]
        pub crate_size: Option<u64>,
        #[cfg(all_fields)]
        pub published_by: Option<UserId>,
        #[cfg(all_fields)]
        pub has_lib: bool,
    }
}

impl VersionRow<'_> {
    /// # Panics
    ///
    /// Panics if the features JSON in the database is malformed
    #[must_use]
    pub fn features(&self) -> BTreeMap<String, Vec<String>> {
        serde_json::from_str(self.features).expect("invalid data in features field")
    }

    #[cfg(all_fields)]
    pub fn categories(&self) -> Vec<String> {
        todo!()
    }

    #[cfg(all_fields)]
    pub fn keywords(&self) -> Vec<String> {
        todo!()
    }

    #[cfg(all_fields)]
    #[must_use]
    pub fn repository(&self) -> Option<Url> {
        if self.repository.is_empty() {
            None
        } else {
            Some(Url::parse(self.repository).expect("invalid URL in repository field"))
        }
    }

    /// # Panics
    ///
    /// Panics if the homepage URL in the database is malformed
    #[must_use]
    pub fn homepage(&self) -> Option<Url> {
        if self.homepage.is_empty() {
            None
        } else {
            Some(Url::parse(self.homepage).expect("invalid URL in homepage field"))
        }
    }

    /// # Panics
    ///
    /// Panics if the documentation URL in the database is malformed
    #[must_use]
    pub fn documentation(&self) -> Option<Url> {
        if self.documentation.is_empty() {
            None
        } else {
            Some(Url::parse(self.documentation).expect("invalid URL in documentation field"))
        }
    }

    #[must_use]
    pub const fn edition(&self) -> Option<RustEdition> {
        match self.edition {
            None => None,
            Some(2015) => Some(RustEdition::Edition2015),
            Some(2018) => Some(RustEdition::Edition2018),
            Some(2021) => Some(RustEdition::Edition2021),
            Some(2024) => Some(RustEdition::Edition2024),
            _ => Some(RustEdition::Unknown),
        }
    }
}

define_table! {
    versions {
        fn write_row(csv_row: &CsvVersionRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            let _ = serde_json::from_str::<BTreeMap<String, Vec<String>>>(csv_row.features).into_app_err("invalid feature map")?;

            writer.write_str_as_u64(csv_row.id)?;
            writer.write_str_as_u64(csv_row.crate_id)?;
            writer.write_str_as_version(csv_row.num)?;
            writer.write_str_as_u64(csv_row.downloads)?;
            writer.write_optional_str_as_u64(csv_row.edition)?;
            writer.write_str_as_datetime(csv_row.created_at)?;
            writer.write_str_as_datetime(csv_row.updated_at)?;
            writer.write_str(csv_row.description)?;
            writer.write_str(csv_row.features)?;
            writer.write_str(csv_row.license)?;
            writer.write_str(csv_row.rust_version)?;
            writer.write_str_as_bool(csv_row.yanked)?;

            if let Err(e) = writer.write_str_as_url(csv_row.documentation) {
                log::debug!(target: LOG_TARGET,
                    "invalid documentation URL in versions table for version {} (crate '{}'): {}",
                    csv_row.num,
                    csv_row.crate_id,
                    e
                );
                writer.write_str("")?;
            }

            if let Err(e) = writer.write_str_as_url(csv_row.homepage) {
                log::debug!(target: LOG_TARGET,
                    "invalid homepage URL in versions table for version {} (crate '{}'): {}",
                    csv_row.num,
                    csv_row.crate_id,
                    e
                );
                writer.write_str("")?;
            }

            #[cfg(all_fields)]
            {
                writer.write_pg_array_as_str_vec(csv_row.categories)?;
                writer.write_pg_array_as_str_vec(csv_row.keywords)?;
                if let Err(e) = writer.write_str_as_url(csv_row.repository) {
                    log::debug!(target: LOG_TARGET,
                        "invalid repository URL in versions table for version {} (crate '{}'): {}",
                        csv_row.num,
                        csv_row.crate_id,
                        e
                    );
                    writer.write_str("")?;
                }
                writer.write_optional_str(csv_row.links)?;
                writer.write_str(csv_row.bin_names)?;
                writer.write_optional_str(csv_row.checksum)?;
                writer.write_optional_str_as_u64(csv_row.crate_size)?;
                writer.write_optional_str_as_u64(csv_row.published_by)?;
                writer.write_str_as_bool(csv_row.has_lib)?;
            }

            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> VersionRow<'a> {
            VersionRow {
                id: VersionId(reader.read_u64()),
                crate_id: CrateId(reader.read_u64()),
                num: reader.read_version(),
                downloads: reader.read_u64(),
                edition: reader.read_optional_u64(),
                created_at: reader.read_datetime(),
                updated_at: reader.read_datetime(),
                description: reader.read_str(),
                features: reader.read_str(),
                license: reader.read_str(),
                rust_version: reader.read_str(),
                yanked: reader.read_bool(),
                documentation: reader.read_str(),
                homepage: reader.read_str(),
                #[cfg(all_fields)]
                categories: reader.read_str_vec(),
                #[cfg(all_fields)]
                keywords: reader.read_str_vec(),
                #[cfg(all_fields)]
                repository: reader.read_str(),
                #[cfg(all_fields)]
                links: reader.read_str(),
                #[cfg(all_fields)]
                bin_names: reader.read_str(),
                #[cfg(all_fields)]
                checksum: reader.read_str(),
                #[cfg(all_fields)]
                crate_size: reader.read_optional_u64(),
                #[cfg(all_fields)]
                published_by: reader.read_optional_u64().map(UserId),
                #[cfg(all_fields)]
                has_lib: reader.read_bool(),
            }
        }
    }
}
