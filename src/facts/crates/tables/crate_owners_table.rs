use super::{CrateId, TeamId, UserId, define_rows, define_table};
use crate::Result;
#[cfg(all_fields)]
use chrono::{DateTime, Utc};
use ohno::bail;

define_rows! {
    CrateOwnerRow {
        pub crate_id: CrateId,
        owner_kind: u64,
        owner_id: u64,

        #[cfg(all_fields)]
        pub created_at: DateTime<Utc>,

        #[cfg(all_fields)]
        pub created_by: Option<UserId>,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerKind {
    User(UserId),
    Team(TeamId),
}

impl CrateOwnerRow {
    #[must_use]
    pub fn owner(&self) -> OwnerKind {
        match self.owner_kind {
            0 => OwnerKind::User(UserId(self.owner_id)),
            1 => OwnerKind::Team(TeamId(self.owner_id)),
            _ => unreachable!("invalid owner_kind: {}", self.owner_kind),
        }
    }
}

define_table! {
    crate_owners {
        fn write_row(csv_row: &CsvCrateOwnerRow<'a>, writer: &mut RowWriter<impl Write>) -> Result<()> {
            if csv_row.owner_kind != "0" && csv_row.owner_kind != "1" {
                bail!("invalid owner_kind: {}", csv_row.owner_kind);
            }

            writer.write_str_as_u64(csv_row.crate_id)?;
            writer.write_str_as_u64(csv_row.owner_kind)?;
            writer.write_str_as_u64(csv_row.owner_id)?;

            #[cfg(all_fields)]
            {
                writer.write_str_as_datetime(csv_row.created_at)?;
                writer.write_optional_str_as_u64(csv_row.created_by)?;
            }

            Ok(())
        }

        fn read_row<'a>(reader: &mut RowReader<'a>) -> CrateOwnerRow {
            CrateOwnerRow {
                crate_id: CrateId(reader.read_u64()),
                owner_kind: reader.read_u64(),
                owner_id: reader.read_u64(),

                #[cfg(all_fields)]
                created_at: reader.read_datetime(),

                #[cfg(all_fields)]
                created_by: reader.read_optional_u64().map(UserId),
            }
        }
    }
}
