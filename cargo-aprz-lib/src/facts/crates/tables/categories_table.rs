use super::{CategoryId, define_rows, define_table};
#[cfg(all_fields)]
use chrono::{DateTime, Utc};

define_rows! {
    CategoryRow<'a> {
        pub id: CategoryId,
        #[allow(dead_code)]
        pub category: &'a str,
        pub slug: &'a str,
        #[cfg(all_fields)]
        pub description: &'a str,
        #[cfg(all_fields)]
        pub crates_cnt: u64,
        #[cfg(all_fields)]
        pub created_at: DateTime<Utc>,
        #[cfg(all_fields)]
        pub path: &'a str,
    }
}

define_table! {
    categories {
        fn write_row(csv_row: &CsvCategoryRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.id)?;
            writer.write_str(csv_row.category)?;
            writer.write_str(csv_row.slug)?;

            #[cfg(all_fields)]
            {
                writer.write_str(csv_row.description)?;
                writer.write_str_as_u64(csv_row.crates_cnt)?;
                writer.write_str_as_datetime(csv_row.created_at)?;
                writer.write_str(csv_row.path)?;
            }

            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> CategoryRow<'a> {
            CategoryRow {
                id: CategoryId(reader.read_u64()),
                category: reader.read_str(),
                slug: reader.read_str(),
                #[cfg(all_fields)]
                description: reader.read_str(),
                #[cfg(all_fields)]
                crates_cnt: reader.read_u64(),
                #[cfg(all_fields)]
                created_at: reader.read_datetime(),
                #[cfg(all_fields)]
                path: reader.read_str(),
            }
        }
    }
}
