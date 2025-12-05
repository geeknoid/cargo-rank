use super::{CrateId, define_rows, define_table};
use chrono::{DateTime, Utc};
use url::Url;

/// Log target for crates table
const LOG_TARGET: &str = "    crates";

define_rows! {
    CrateRow<'a> {
        pub id: CrateId,
        pub name: &'a str,
        pub created_at: DateTime<Utc>,
        pub updated_at: DateTime<Utc>,
        repository: &'a str,
        #[cfg(all_fields)]
        pub description: &'a str,
        #[cfg(all_fields)]
        documentation: &'a str,
        #[cfg(all_fields)]
        homepage: &'a str,
        #[cfg(all_fields)]
        pub readme: &'a str,
        #[cfg(all_fields)]
        pub max_features: Option<u64>,
        #[cfg(all_fields)]
        pub max_upload_size: Option<u64>,
        #[cfg(all_fields)]
        pub trustpub_only: bool,
    }
}

impl CrateRow<'_> {
    /// # Panics
    ///
    /// Panics if the repository URL in the database is malformed
    #[must_use]
    pub fn repository(&self) -> Option<Url> {
        if self.repository.is_empty() {
            None
        } else {
            Some(Url::parse(self.repository).expect("invalid URL in repository field"))
        }
    }

    #[cfg(all_fields)]
    #[must_use]
    pub fn homepage(&self) -> Option<Url> {
        if self.homepage.is_empty() {
            None
        } else {
            Some(Url::parse(self.homepage).expect("invalid URL in homepage field"))
        }
    }

    #[cfg(all_fields)]
    #[must_use]
    pub fn documentation(&self) -> Option<Url> {
        if self.documentation.is_empty() {
            None
        } else {
            Some(Url::parse(self.documentation).expect("invalid URL in documentation field"))
        }
    }
}

define_table! {
    crates {
        fn write_row(csv_row: &CsvCrateRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str(csv_row.name)?;
            writer.write_str_as_u64(csv_row.id)?;
            writer.write_str_as_datetime(csv_row.created_at)?;

            if let Err(e) = writer.write_str_as_url(csv_row.repository) {
                log::debug!(target: LOG_TARGET,
                    "invalid repository URL in crates table for crate '{}': {}",
                    csv_row.name,
                    e
                );
                writer.write_str("")?;
            }

            writer.write_str_as_datetime(csv_row.updated_at)?;

            #[cfg(all_fields)]
            {
                writer.write_str(csv_row.description)?;

                if let Err(e) = writer.write_str_as_url(csv_row.documentation) {
                    log::debug!(target: LOG_TARGET,
                        "invalid documentation URL in crates table for crate '{}': {}",
                        csv_row.name,
                        e
                    );
                    writer.write_str("")?;
                }

                if let Err(e) = writer.write_str_as_url(csv_row.homepage) {
                    log::debug!(target: LOG_TARGET,
                        "invalid homepage URL in crates table for crate '{}': {}",
                        csv_row.name,
                        e
                    );
                    writer.write_str("")?;
                }

                writer.write_str(csv_row.readme)?;
                writer.write_optional_str_as_u64(csv_row.max_features)?;
                writer.write_optional_str_as_u64(csv_row.max_upload_size)?;
                writer.write_str_as_bool(csv_row.trustpub_only)?;
            }

            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> CrateRow<'a> {
            CrateRow {
                name: reader.read_str(),
                id: CrateId(reader.read_u64()),
                created_at: reader.read_datetime(),
                repository: reader.read_str(),
                updated_at: reader.read_datetime(),

                #[cfg(all_fields)]
                description: reader.read_str(),
                #[cfg(all_fields)]
                documentation: reader.read_str(),
                #[cfg(all_fields)]
                homepage: reader.read_str(),
                #[cfg(all_fields)]
                readme: reader.read_str(),
                #[cfg(all_fields)]
                max_features: reader.read_optional_u64(),
                #[cfg(all_fields)]
                max_upload_size: reader.read_optional_u64(),
                #[cfg(all_fields)]
                trustpub_only: reader.read_bool(),
            }
        }
    }
}
