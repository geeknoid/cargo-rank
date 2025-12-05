use super::{CrateId, KeywordId, define_rows, define_table};

define_rows! {
    CratesKeywordsRow {
        pub crate_id: CrateId,
        pub keyword_id: KeywordId,
    }
}

define_table! {
    crates_keywords {
        fn write_row(csv_row: &CsvCratesKeywordsRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.crate_id)?;
            writer.write_str_as_u64(csv_row.keyword_id)?;
            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> CratesKeywordsRow {
            CratesKeywordsRow {
                crate_id: CrateId(reader.read_u64()),
                keyword_id: KeywordId(reader.read_u64()),
            }
        }
    }
}
