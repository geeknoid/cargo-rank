use super::{define_rows, define_table};

define_rows! {
    ReservedCrateNamesRow<'a> {
        pub name: &'a str,
    }
}

define_table! {
    reserved_crate_names {
        fn write_row(csv_row: &CsvReservedCrateNamesRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str(csv_row.name)?;
            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> ReservedCrateNamesRow<'a> {
            ReservedCrateNamesRow { name: reader.read_str() }
        }
    }
}
