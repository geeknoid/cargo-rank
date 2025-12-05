use super::{TeamId, define_rows, define_table};
define_rows! {
    TeamRow<'a> {
        pub id: TeamId,
        pub login: &'a str,
        pub name: &'a str,
        #[cfg(all_fields)]
        pub org_id: Option<u64>,
        #[cfg(all_fields)]
        pub avatar: &'a str,
        #[cfg(all_fields)]
        pub github_id: u64,
    }
}

define_table! {
    teams {
        fn write_row(csv_row: &CsvTeamRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            writer.write_str_as_u64(csv_row.id)?;
            writer.write_str(csv_row.login)?;
            writer.write_str(csv_row.name)?;

            #[cfg(all_fields)]
            {
                writer.write_optional_str_as_u64(csv_row.org_id)?;
                writer.write_str(csv_row.avatar)?;
                writer.write_str_as_u64(csv_row.github_id)?;
            }

            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> TeamRow<'a> {
            TeamRow {
                id: TeamId(reader.read_u64()),
                login: reader.read_str(),
                name: reader.read_str(),
                #[cfg(all_fields)]
                org_id: reader.read_optional_u64(),
                #[cfg(all_fields)]
                avatar: reader.read_str(),
                #[cfg(all_fields)]
                github_id: reader.read_u64(),
            }
        }
    }
}
