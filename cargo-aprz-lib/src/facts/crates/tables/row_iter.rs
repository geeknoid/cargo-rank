use super::RowReader;
use core::fmt::{Debug, Formatter, Result as FmtResult};
use core::ptr::from_ref;

pub struct RowIter<'a, Row, Index> {
    reader: RowReader<'a>,
    read_fn: fn(&mut RowReader<'a>) -> Row,
    rows_remaining: u64,
    _phantom: core::marker::PhantomData<Index>,
}

impl<'a, Row, Index> RowIter<'a, Row, Index> {
    pub(super) const fn new(reader: RowReader<'a>, read_fn: fn(&mut RowReader<'a>) -> Row, num_rows: u64) -> Self {
        Self {
            reader,
            read_fn,
            rows_remaining: num_rows,
            _phantom: core::marker::PhantomData,
        }
    }
}

impl<Row, Index: From<usize>> Iterator for RowIter<'_, Row, Index> {
    type Item = (Row, Index);

    fn next(&mut self) -> Option<Self::Item> {
        if self.rows_remaining == 0 {
            return None;
        }

        let row_position = self.reader.position();
        let row = (self.read_fn)(&mut self.reader);
        self.rows_remaining -= 1;
        let index = Index::from(row_position);

        Some((row, index))
    }

    #[expect(clippy::cast_possible_truncation, reason = "Tables won't exceed usize::MAX entries in practice")]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.rows_remaining as usize;
        (remaining, Some(remaining))
    }
}

impl<Row, Index> Debug for RowIter<'_, Row, Index> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("RowIter")
            .field("reader", &self.reader)
            .field("read_fn", &from_ref(&self.read_fn))
            .field("rows_remaining", &self.rows_remaining)
            .finish()
    }
}
