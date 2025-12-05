use super::RowIter;
use super::{RowReader, RowWriter};
use crate::Result;
use chrono::{DateTime, TimeZone, Utc};
use core::time::Duration;
use csv::{Reader, StringRecord};
use mmap_rs::{Mmap, MmapFlags, MmapOptions};
use ohno::IntoAppError;
use serde::de::Deserialize;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read as IoRead, Seek, SeekFrom, Write};
use std::path::Path;

#[cfg(all_fields)]
const FORMAT_MAGIC: u64 = 0xC0DE_C0DE_C0DE_000C;

#[cfg(not(all_fields))]
const FORMAT_MAGIC: u64 = 0xC0DE_C0DE_C0DE_000D;

pub const TABLE_HEADER_SIZE: usize = 24; // 8 bytes magic + 8 bytes count + 8 bytes timestamp

pub trait Table: Sized {
    type CsvRow<'a>: Deserialize<'a>;
    type Row<'a>
    where
        Self: 'a;
    type Index: Copy + From<usize>;

    // Constants
    const CSV_NAME: &'static str;
    const TABLE_NAME: &'static str;

    // CSV serialization
    fn write_row(csv_row: &Self::CsvRow<'_>, writer: &mut RowWriter<impl Write>) -> Result<()>;
    fn read_row<'a>(reader: &mut RowReader<'a>) -> Self::Row<'a>;

    // Table construction and opening
    fn open_with(mmap: Mmap, max_ttl: Duration, now: DateTime<Utc>) -> Result<Self>;

    fn open(tables_root: impl AsRef<Path>, max_ttl: Duration, now: DateTime<Utc>) -> Result<Self> {
        let path = tables_root.as_ref().join(Self::TABLE_NAME);
        let file = File::open(&path).into_app_err_with(|| format!("unable to open table file: {}", path.display()))?;

        // Get file size for mapping
        let metadata = file.metadata().into_app_err("unable to get file metadata")?;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "Table files won't exceed usize::MAX on any supported platform"
        )]
        let file_size = metadata.len() as usize;

        // SAFETY: We have read-only access to the file for the duration of the mmap.
        // The file is controlled by this application and won't be modified externally.
        let mmap = unsafe {
            MmapOptions::new(file_size)?
                .with_flags(MmapFlags::TRANSPARENT_HUGE_PAGES | MmapFlags::SEQUENTIAL)
                .with_file(&file, 0)
                .map()
                .into_app_err("unable to memory-map table file")?
        };

        Self::open_with(mmap, max_ttl, now)
    }

    fn create_table(tables_root: impl AsRef<Path>, csv_entry: impl IoRead, now: DateTime<Utc>) -> Result<File> {
        let tables_root = tables_root.as_ref();
        let path = tables_root.join(Self::TABLE_NAME);

        // Open with read+write permissions so we can write AND memory-map with the same handle
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .into_app_err_with(|| format!("unable to create table file: {}", path.display()))?;

        // Use a 1MB buffer for better performance with large tables
        let mut buf_writer = BufWriter::with_capacity(1024 * 1024, file);

        // Write header placeholder
        buf_writer.write_all(&[0u8; TABLE_HEADER_SIZE])?;

        let mut csv_reader = Reader::from_reader(csv_entry);
        let mut row_writer = RowWriter::new(&mut buf_writer);

        let headers = csv_reader.headers()?.clone();
        let mut record = StringRecord::new();
        while csv_reader.read_record(&mut record)? {
            let row = record.deserialize(Some(&headers))?;
            Self::write_row(&row, &mut row_writer)?;
            row_writer.row_done()?;
        }

        let count = row_writer.row_count();
        let timestamp = now.timestamp().cast_unsigned();

        // padding to ensure vlen never tries to read past EOF
        buf_writer.write_all(&[0u8; 10])?;

        // Go back and write the header
        let _ = buf_writer.seek(SeekFrom::Start(0))?;
        buf_writer.write_all(&FORMAT_MAGIC.to_le_bytes())?;
        buf_writer.write_all(&count.to_le_bytes())?;
        buf_writer.write_all(&timestamp.to_le_bytes())?;
        buf_writer.flush()?;

        // Return the file handle - it's open with read+write so it can be memory-mapped
        let file = buf_writer.into_inner()?;
        file.sync_all()?;
        Ok(file)
    }

    // Runtime data access
    fn iter(&self) -> RowIter<'_, Self::Row<'_>, Self::Index>;
    fn get(&self, index: Self::Index) -> Self::Row<'_>;
    fn len(&self) -> usize;
    fn timestamp(&self) -> DateTime<Utc>;
}

/// Generates a table struct, index type, and implementation from a `snake_case` name and row conversion functions.
///
/// Creates:
/// - `{Name}Table` - Main table struct with memory-mapped file access
/// - `{Name}TableIndex` - Type-safe index for accessing rows
/// - Implementation of `Table` trait
/// - File name constants derived from the base name (`{name}.csv`, `{name}.table`)
///
/// See `crates_table.rs`, `versions_table.rs`, or any table file for usage examples.
macro_rules! define_table {
    (
        $name_snake:ident {
            fn write_row($csv_param:ident: &$csv_ty:ty, $writer:ident: &mut RowWriter<impl Write>) -> Result<()>
                $write_body:block

            fn read_row<'a>($reader:ident: &mut RowReader<'a>) -> $row_result:ty
                $read_body:block
        }
    ) => {
        pastey::paste! {
            // Derive all names from the snake_case base name
            // Table struct: snake_case -> PascalCase + "Table"
            // Row struct: snake_case -> PascalCase + "Row"
            // Index struct: snake_case -> PascalCase + "TableIndex"
            // CSV file: snake_case + ".csv"
            // Table file: snake_case + ".table"

            #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
            pub struct [<$name_snake:camel Table Index>](usize);

            impl From<usize> for [<$name_snake:camel Table Index>] {
                fn from(value: usize) -> Self {
                    Self(value)
                }
            }

            #[derive(Debug)]
            pub struct [<$name_snake:camel Table>] {
                mmap: mmap_rs::Mmap,
                count: u64,
                timestamp: chrono::DateTime<chrono::Utc>,
            }

            impl super::Table for [<$name_snake:camel Table>] {
                type CsvRow<'a> = $csv_ty;
                type Row<'a> = $row_result;
                type Index = [<$name_snake:camel Table Index>];

                const CSV_NAME: &'static str = concat!(stringify!($name_snake), ".csv");
                const TABLE_NAME: &'static str = concat!(stringify!($name_snake), ".table");

                fn write_row($csv_param: &Self::CsvRow<'_>, $writer: &mut super::RowWriter<impl std::io::Write>) -> crate::Result<()>
                    $write_body

                fn read_row<'a>($reader: &mut super::RowReader<'a>) -> Self::Row<'a>
                    $read_body

                fn open_with(mmap: mmap_rs::Mmap, max_ttl: core::time::Duration, now: chrono::DateTime<chrono::Utc>) -> crate::Result<Self> {
                    let (count, timestamp) = super::validate_table_header(&mmap, max_ttl, now)?;
                    Ok(Self { mmap, count, timestamp })
                }

                fn iter(&self) -> super::RowIter<'_, Self::Row<'_>, Self::Index> {
                    super::RowIter::new(
                        super::RowReader::new(&self.mmap[super::TABLE_HEADER_SIZE..]),
                        Self::read_row,
                        self.count
                    )
                }

                fn get(&self, index: Self::Index) -> Self::Row<'_> {
                    let mut reader = super::RowReader::new(&self.mmap[super::TABLE_HEADER_SIZE + index.0..]);
                    Self::read_row(&mut reader)
                }

                #[expect(clippy::cast_possible_truncation, reason = "Tables won't exceed usize::MAX entries in practice")]
                fn len(&self) -> usize {
                    self.count as usize
                }

                fn timestamp(&self) -> chrono::DateTime<chrono::Utc> {
                    self.timestamp
                }
            }
        }
    };
}

/// Generates both CSV row and table row structs from a single row definition.
///
/// Creates:
/// - `Csv{RowName}` - Struct for deserializing from CSV, with all fields as `&'a str`
/// - `{RowName}` - Table row struct with the specified field types
///
/// The macro has two variants:
/// - With `<'a>` lifetime parameter - generates `#[derive(Clone)]` row struct
/// - Without lifetime - generates `#[derive(Clone, Copy)]` row struct
///
/// See `categories_table.rs`, `users_table.rs`, or `versions_table.rs` for usage examples.
macro_rules! define_rows {
    (
        $row_name:ident<'a> {
            $(
                $(#[$field_meta:meta])*
                $vis:vis $field:ident: $field_type:ty
            ),* $(,)?
        }
    ) => {
        pastey::paste! {
            #[derive(Debug, serde::Deserialize)]
            pub struct [<Csv $row_name>]<'a> {
                $(
                    $(#[$field_meta])*
                    #[serde(borrow)]
                    $field: &'a str,
                )*
            }
        }

        #[derive(Debug, Clone)]
        pub struct $row_name<'a> {
            $(
                $(#[$field_meta])*
                $vis $field: $field_type,
            )*
        }
    };

    // Variant for rows without lifetimes
    (
        $row_name:ident {
            $(
                $(#[$field_meta:meta])*
                $vis:vis $field:ident: $field_type:ty
            ),* $(,)?
        }
    ) => {
        pastey::paste! {
            #[derive(Debug, serde::Deserialize)]
            pub struct [<Csv $row_name>]<'a> {
                $(
                    $(#[$field_meta])*
                    #[serde(borrow)]
                    $field: &'a str,
                )*
            }
        }

        #[derive(Debug, Clone, Copy)]
        pub struct $row_name {
            $(
                $(#[$field_meta])*
                $vis $field: $field_type,
            )*
        }
    };
}

pub(crate) use define_rows;
pub(crate) use define_table;

pub fn validate_table_header(mmap: &Mmap, max_ttl: Duration, now: DateTime<Utc>) -> Result<(u64, DateTime<Utc>)> {
    use ohno::bail;

    if mmap.len() < TABLE_HEADER_SIZE {
        bail!("invalid table: file too short (need at least 24 bytes for header)");
    }
    assert!(mmap.len() >= TABLE_HEADER_SIZE, "Length check above guarantees at least 24 bytes");
    assert!(mmap.len() > 23, "mmap length sufficient for all header indexing operations");

    // Validate format magic identifier
    let magic_bytes = mmap[0..8].try_into()?;
    let magic = u64::from_le_bytes(magic_bytes);
    if magic != FORMAT_MAGIC {
        bail!("invalid table format: expected magic 0x{FORMAT_MAGIC:016X}, found 0x{magic:016X}. Database may need regeneration.");
    }

    // Read row count
    let count_bytes = mmap[8..16].try_into()?;
    let count = u64::from_le_bytes(count_bytes);

    // Read and validate creation timestamp
    let timestamp_bytes = mmap[16..24].try_into()?;
    let table_timestamp = u64::from_le_bytes(timestamp_bytes);

    // Check TTL
    let now_secs = now.timestamp().cast_unsigned();
    let age_seconds = now_secs.saturating_sub(table_timestamp);
    let age = Duration::from_secs(age_seconds);

    if age > max_ttl {
        bail!("table is stale: age {}s exceeds TTL {}s", age.as_secs(), max_ttl.as_secs());
    }

    let dt = Utc
        .timestamp_opt(i64::try_from(table_timestamp).into_app_err("timestamp out of range for i64")?, 0)
        .single()
        .ok_or_else(|| ohno::app_err!("invalid or out-of-range timestamp"))?;

    Ok((count, dt))
}
