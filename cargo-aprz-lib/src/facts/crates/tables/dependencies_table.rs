use super::{CrateId, VersionId, define_rows, define_table};
use crate::Result;
use ohno::bail;
use serde::Deserialize;

define_rows! {
    DependencyRow<'a> {
        pub version_id: VersionId,
        pub crate_id: CrateId,

        #[allow(dead_code, reason = "Needed for Compilcated Reasons (TM)")]
        pub features: Vec<&'a str>,

        #[cfg(all_fields)]
        pub id: super::DependencyId,
        #[cfg(all_fields)]
        kind: u64,
        #[cfg(all_fields)]
        pub default_features: bool,
        #[cfg(all_fields)]
        pub explicit_name: &'a str,
        #[cfg(all_fields)]
        pub optional: bool,
        #[cfg(all_fields)]
        pub req: &'a str,
        #[cfg(all_fields)]
        pub target: &'a str,
    }
}

#[cfg(all_fields)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyKind {
    Normal,
    Build,
    Dev,
}

impl DependencyRow<'_> {
    #[cfg(all_fields)]
    pub fn kind(&self) -> DependencyKind {
        match self.kind {
            0 => DependencyKind::Normal,
            1 => DependencyKind::Build,
            2 => DependencyKind::Dev,
            _ => unreachable!("invalid dependency kind: {}", self.kind),
        }
    }
}

define_table! {
    dependencies {
        fn write_row(csv_row: &CsvDependencyRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.version_id)?;
            writer.write_str_as_u64(csv_row.crate_id)?;
            writer.write_pg_array_as_str_vec(csv_row.features)?;

            #[cfg(all_fields)]
            {
                if csv_row.kind != "0" && csv_row.kind != "1" && csv_row.kind != "2" {
                    bail!("invalid dependency kind: {}", csv_row.kind);
                }

                writer.write_str_as_u64(csv_row.id)?;
                writer.write_str_as_u64(csv_row.kind)?;
                writer.write_str_as_bool(csv_row.default_features)?;
                writer.write_str(csv_row.explicit_name)?;
                writer.write_str_as_bool(csv_row.optional)?;
                writer.write_str(csv_row.req)?;
                writer.write_str(csv_row.target)?;
            }

            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> DependencyRow<'a> {
            DependencyRow {
                version_id: VersionId(reader.read_u64()),
                crate_id: CrateId(reader.read_u64()),
                features: reader.read_str_vec(),

                #[cfg(all_fields)]
                id: super::DependencyId(reader.read_u64()),

                #[cfg(all_fields)]
                kind: reader.read_u64(),

                #[cfg(all_fields)]
                default_features: reader.read_bool(),

                #[cfg(all_fields)]
                explicit_name: reader.read_str(),

                #[cfg(all_fields)]
                optional: reader.read_bool(),

                #[cfg(all_fields)]
                req: reader.read_str(),

                #[cfg(all_fields)]
                target: reader.read_str(),
            }
        }
    }
}
