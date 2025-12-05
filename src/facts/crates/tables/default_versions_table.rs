use super::{CrateId, VersionId, define_rows, define_table};

define_rows! {
    DefaultVersionRow {
        pub crate_id: CrateId,
        pub num_versions: u64,
        pub version_id: VersionId,
    }
}

define_table! {
    default_versions {
        fn write_row(csv_row: &CsvDefaultVersionRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.crate_id)?;
            writer.write_str_as_u64(csv_row.num_versions)?;
            writer.write_str_as_u64(csv_row.version_id)?;
            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> DefaultVersionRow {
            DefaultVersionRow {
                crate_id: CrateId(reader.read_u64()),
                num_versions: reader.read_u64(),
                version_id: VersionId(reader.read_u64()),
            }
        }
    }
}
