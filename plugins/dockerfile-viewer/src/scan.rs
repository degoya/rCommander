//! Lines from a [`Reader`]: each one's first bytes - at most the bytes that get tokens - and the
//! last byte of the whole line that is no blank, which says whether the escape character
//! continues it. A longer line is read through to its end, never held.

use crate::reader::Reader;
use crate::types::{Failure, Source};

/// How much of the source one look takes.
const LOOK: usize = 64 * 1024;

/// What [`Scanner::next_line`] found besides the bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Line {
    /// The last byte of the line that is no space, tab or carriage return.
    pub(crate) tail: Option<u8>,
}

pub(crate) struct Scanner<'a, S: Source> {
    reader: Reader<'a, S>,
}

impl<'a, S: Source> Scanner<'a, S> {
    pub(crate) fn new(reader: Reader<'a, S>) -> Self {
        Self { reader }
    }

    /// The offset of the next line's first byte.
    pub(crate) fn offset(&self) -> u64 {
        self.reader.offset()
    }

    /// Reads the next line into `raw`, without its line break and cut to `cap` bytes, the cut
    /// moved back before a character it would split; `None` at the end of the source.
    pub(crate) fn next_line(
        &mut self,
        cap: usize,
        raw: &mut Vec<u8>,
    ) -> Result<Option<Line>, Failure> {
        raw.clear();
        let mut tail = None;
        let (mut any, mut capped) = (false, false);
        loop {
            let window = self.reader.look(LOOK)?;
            if window.is_empty() {
                break;
            }
            any = true;
            let newline = window.iter().position(|byte| *byte == b'\n');
            let body = window
                .get(..newline.unwrap_or(window.len()))
                .unwrap_or_default();
            if let Some(last) = body
                .iter()
                .rev()
                .find(|byte| !matches!(**byte, b' ' | b'\t' | b'\r'))
            {
                tail = Some(*last);
            }
            let room = cap.saturating_sub(raw.len());
            capped |= body.len() > room;
            raw.extend_from_slice(body.get(..body.len().min(room)).unwrap_or_default());
            let taken = body.len() + usize::from(newline.is_some());
            self.reader.bump(taken);
            if newline.is_some() {
                break;
            }
        }
        if !any {
            return Ok(None);
        }
        if capped {
            // A cut inside a character would make it U+FFFD in the tokens' text but not in the
            // text written: the cut ends at an ASCII byte instead.
            while raw.last().is_some_and(|byte| *byte >= 0x80) {
                raw.pop();
            }
        }
        Ok(Some(Line { tail }))
    }
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
