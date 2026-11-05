//! A buffered reader over a [`Source`]: windows of `chunk` bytes through `read-at`, so a call
//! never holds more of the file than one window and one line, and `should-stop` is asked before
//! each. The batch viewer's reader, with the line break of the file's encoding: a `\n` byte, or
//! the code unit `0x000A` of UTF-16 - line breaks are whole units in every encoding read, so a
//! line's bytes decode on their own.
//!
//! In UTF-16 a line break is found only at an even distance from the text's first byte: windows
//! are a power of two long and every line starts after a whole unit, so the pairs stay aligned.

use rc_plugin_text::Encoding;

use crate::types::{Failure, FailureKind, Source};

/// How a line break is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Unit {
    Byte,
    Utf16Le,
    Utf16Be,
}

impl Unit {
    fn of(encoding: Encoding) -> Self {
        match encoding {
            Encoding::Utf16Le => Self::Utf16Le,
            Encoding::Utf16Be => Self::Utf16Be,
            _ => Self::Byte,
        }
    }

    /// `\n` in this unit.
    fn newline(self) -> &'static [u8] {
        match self {
            Self::Byte => b"\n",
            Self::Utf16Le => b"\n\0",
            Self::Utf16Be => b"\0\n",
        }
    }

    /// `\r` in this unit.
    fn carriage_return(self) -> &'static [u8] {
        match self {
            Self::Byte => b"\r",
            Self::Utf16Le => b"\r\0",
            Self::Utf16Be => b"\0\r",
        }
    }

    /// The first line break in `bytes`, which start on a unit.
    fn find(self, bytes: &[u8]) -> Option<usize> {
        match self {
            Self::Byte => bytes.iter().position(|byte| *byte == b'\n'),
            _ => bytes
                .as_chunks::<2>()
                .0
                .iter()
                .position(|pair| pair.as_slice() == self.newline())
                .map(|unit| unit * 2),
        }
    }
}

pub(crate) struct Reader<'a, S: Source> {
    src: &'a mut S,
    size: u64,
    chunk: u32,
    buf: Vec<u8>,
    buf_start: u64,
    pos: usize,
    unit: Unit,
}

impl<'a, S: Source> Reader<'a, S> {
    /// A reader from `offset`, splitting lines as `encoding` writes them.
    pub(crate) fn new(src: &'a mut S, offset: u64, chunk: u32, encoding: Encoding) -> Self {
        let size = src.size();
        Self {
            src,
            size,
            chunk,
            buf: Vec::new(),
            buf_start: offset,
            pos: 0,
            unit: Unit::of(encoding),
        }
    }

    /// Starts with `window`, the bytes from `window_start` already read: the reader's offset
    /// lies within it or at its end.
    pub(crate) fn with_window(mut self, window_start: u64, window: Vec<u8>) -> Self {
        let offset = self.buf_start;
        let pos = usize::try_from(offset.saturating_sub(window_start)).unwrap_or(usize::MAX);
        if offset >= window_start && pos <= window.len() {
            self.buf = window;
            self.buf_start = window_start;
            self.pos = pos;
        }
        self
    }

    /// The offset of the next byte.
    pub(crate) fn offset(&self) -> u64 {
        self.buf_start + self.pos as u64
    }

    /// Reads the next window; false at the end of the source.
    fn refill(&mut self) -> Result<bool, Failure> {
        let at = self.offset();
        if at >= self.size {
            return Ok(false);
        }
        if self.src.should_stop() {
            return Err(Failure::of(FailureKind::Cancelled));
        }
        let length =
            u32::try_from((self.size - at).min(u64::from(self.chunk))).unwrap_or(self.chunk);
        let mut bytes = self.src.read_at(at, length)?;
        if bytes.is_empty() {
            // A source shorter than announced ends here rather than looping.
            self.size = at;
            return Ok(false);
        }
        bytes.truncate(length as usize);
        self.src.progress(at + bytes.len() as u64, Some(self.size));
        self.buf = bytes;
        self.buf_start = at;
        self.pos = 0;
        Ok(true)
    }

    /// Reads the next line into `line`: without its line break (and a `\r` before it), at most
    /// `cap` bytes of it - `cap` even, so a UTF-16 unit is never cut; the rest of a longer line
    /// is read past. `None` at the end of the source, else whether the line was longer than
    /// `cap` - its `\r` then is not taken off.
    pub(crate) fn read_line(
        &mut self,
        cap: usize,
        line: &mut Vec<u8>,
    ) -> Result<Option<bool>, Failure> {
        line.clear();
        if self.pos >= self.buf.len() && !self.refill()? {
            return Ok(None);
        }
        let width = self.unit.newline().len();
        let mut length = 0usize;
        loop {
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            let (take, newline) = match self.unit.find(rest) {
                Some(at) => (at, width),
                None => (rest.len(), 0),
            };
            let room = cap.saturating_sub(line.len());
            line.extend_from_slice(rest.get(..take.min(room)).unwrap_or_default());
            length = length.saturating_add(take);
            self.pos += take + newline;
            if newline > 0 || (self.pos >= self.buf.len() && !self.refill()?) {
                break;
            }
        }
        let capped = length > cap;
        if !capped && line.ends_with(self.unit.carriage_return()) {
            line.truncate(line.len() - width);
        }
        Ok(Some(capped))
    }
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
