use super::{CategoryId, CrateId, define_rows, define_table};

define_rows! {
    CratesCategoriesRow {
        pub crate_id: CrateId,
        pub category_id: CategoryId,
    }
}

define_table! {
    crates_categories {
        fn write_row(csv_row: &CsvCratesCategoriesRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.crate_id)?;
            writer.write_str_as_u64(csv_row.category_id)?;
            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> CratesCategoriesRow {
            CratesCategoriesRow {
                crate_id: CrateId(reader.read_u64()),
                category_id: CategoryId(reader.read_u64()),
            }
        }
    }
}
