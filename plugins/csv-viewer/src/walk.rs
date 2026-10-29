//! Records read one after another from a record's start, for the tree: each field's value
//! unquoted and only as much of it as is asked for.

use crate::machine::{Kind, Machine, Mode};
use crate::reader::Reader;
use crate::types::{Failure, Source};

/// The window the walker reads through.
const WALK_CHUNK: u32 = 64 * 1024;

pub(crate) struct Walker<'a, S: Source> {
    reader: Reader<'a, S>,
    machine: Machine,
}

impl<'a, S: Source> Walker<'a, S> {
    /// A walker at `offset`, which starts a record (or blank lines before one).
    pub(crate) fn new(src: &'a mut S, offset: u64, sep: u8) -> Self {
        Self {
            reader: Reader::new(src, offset, WALK_CHUNK),
            machine: Machine::new(sep, Mode::RecordStart),
        }
    }

    /// Reads the next record: `keep(column)` says how many bytes of a field's value to keep,
    /// `field(column, value)` gets them. Its number of fields; `None` after the last record.
    pub(crate) fn record(
        &mut self,
        keep: &mut dyn FnMut(u64) -> usize,
        field: &mut dyn FnMut(u64, Vec<u8>),
    ) -> Result<Option<u64>, Failure> {
        let mut column = 0u64;
        let mut value = Vec::new();
        let mut limit = keep(0);
        loop {
            let Some(byte) = self.reader.peek()? else {
                if self.machine.finish().is_none() {
                    return Ok(None);
                }
                field(column, value);
                return Ok(Some(column + 1));
            };
            self.reader.bump();
            match self.machine.feed(byte).kind {
                Kind::Blank | Kind::Content(None) => {}
                Kind::Content(Some(byte)) => {
                    if value.len() < limit {
                        value.push(byte);
                    }
                }
                Kind::End { record, .. } => {
                    if record && value.last() == Some(&b'\r') {
                        value.pop();
                    }
                    field(column, std::mem::take(&mut value));
                    column += 1;
                    if record {
                        return Ok(Some(column));
                    }
                    limit = keep(column);
                }
            }
        }
    }

    /// Skips the next record; false after the last.
    pub(crate) fn skip(&mut self) -> Result<bool, Failure> {
        Ok(self.record(&mut |_| 0, &mut |_, _| {})?.is_some())
    }
}
