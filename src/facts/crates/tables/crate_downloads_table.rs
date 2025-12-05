use super::{CrateId, define_rows, define_table};

define_rows! {
    CrateDownloadRow {
        pub crate_id: CrateId,
        pub downloads: u64,
    }
}

define_table! {
    crate_downloads {
        fn write_row(csv_row: &CsvCrateDownloadRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.crate_id)?;
            writer.write_str_as_u64(csv_row.downloads)?;
            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> CrateDownloadRow {
            CrateDownloadRow {
                crate_id: CrateId(reader.read_u64()),
                downloads: reader.read_u64(),
            }
        }
    }
}
