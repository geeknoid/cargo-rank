use super::{UserId, define_rows, define_table};

define_rows! {
    UserRow<'a> {
        pub id: UserId,
        pub gh_login: &'a str,
        pub name: &'a str,
        #[cfg(all_fields)]
        pub gh_id: Option<u64>,
        #[cfg(all_fields)]
        pub gh_avatar: &'a str,
    }
}

define_table! {
    users {
        fn write_row(csv_row: &CsvUserRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.id)?;
            writer.write_str(csv_row.gh_login)?;
            writer.write_str(csv_row.name)?;

            #[cfg(all_fields)]
            {
                let gh_id = if csv_row.gh_id == "-1" {
                    None
                } else {
                    Some(csv_row.gh_id.parse::<u64>()?)
                };

                writer.write_optional_u64(gh_id)?;
                writer.write_str(csv_row.gh_avatar)?;
            }

            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> UserRow<'a> {
            UserRow {
                id: UserId(reader.read_u64()),
                gh_login: reader.read_str(),
                name: reader.read_str(),
                #[cfg(all_fields)]
                gh_id: reader.read_optional_u64(),
                #[cfg(all_fields)]
                gh_avatar: reader.read_str(),
            }
        }
    }
}
