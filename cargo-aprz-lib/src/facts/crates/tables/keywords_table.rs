use super::{KeywordId, define_rows, define_table};
#[cfg(all_fields)]
use chrono::{DateTime, Utc};

define_rows! {
    KeywordRow<'a> {
        pub id: KeywordId,
        pub keyword: &'a str,
        #[cfg(all_fields)]
        pub crates_cnt: u64,
        #[cfg(all_fields)]
        pub created_at: DateTime<Utc>,
    }
}

define_table! {
    keywords {
        fn write_row(csv_row: &CsvKeywordRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.id)?;
            writer.write_str(csv_row.keyword)?;

            #[cfg(all_fields)]
            {
                writer.write_str_as_u64(csv_row.crates_cnt)?;
                writer.write_str_as_datetime(csv_row.created_at)?;
            }

            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> KeywordRow<'a> {
            KeywordRow {
                id: KeywordId(reader.read_u64()),
                keyword: reader.read_str(),

                #[cfg(all_fields)]
                crates_cnt: reader.read_u64(),

                #[cfg(all_fields)]
                created_at: reader.read_datetime(),
            }
        }
    }
}
