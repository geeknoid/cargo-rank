use super::{define_rows, define_table};

define_rows! {
    MetadataRow {
        pub total_downloads: u64,
    }
}

define_table! {
    metadata {
        fn write_row(csv_row: &CsvMetadataRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.total_downloads)?;
            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> MetadataRow {
            MetadataRow {
                total_downloads: reader.read_u64(),
            }
        }
    }
}
