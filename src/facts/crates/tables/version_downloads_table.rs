use super::{VersionId, define_rows, define_table};
use chrono::NaiveDate;

define_rows! {
    VersionDownloadRow {
        pub version_id: VersionId,
        pub downloads: u64,
        pub date: NaiveDate,
    }
}

define_table! {
    version_downloads {
        fn write_row(csv_row: &CsvVersionDownloadRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.version_id)?;
            writer.write_str_as_u64(csv_row.downloads)?;
            writer.write_str_as_date(csv_row.date)?;
            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> VersionDownloadRow {
            VersionDownloadRow {
                version_id: VersionId(reader.read_u64()),
                downloads: reader.read_u64(),
                date: reader.read_date(),
            }
        }
    }
}
