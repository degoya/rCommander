//! A buffered line reader over a [`Source`]: windows of `chunk` bytes through `read-at`, so a call
//! never holds more of the file than one window and one line, and `should-stop` is asked before
//! each. The batch viewer's reader, with lines split in the file's own encoding: a line break is
//! the byte 0x0A, or in UTF-16 the unit U+000A at an even offset from the text's start.

use rc_plugin_text::Encoding;

use crate::types::{Failure, FailureKind, Source};

/// How a line break is written in the file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Newline {
    /// `\n`: UTF-8 and the code pages.
    Byte,
    Utf16Le,
    Utf16Be,
}

impl Newline {
    pub(crate) fn of(encoding: Encoding) -> Self {
        match encoding {
            Encoding::Utf16Le => Self::Utf16Le,
            Encoding::Utf16Be => Self::Utf16Be,
            _ => Self::Byte,
        }
    }

    /// The bytes of one character.
    fn unit(self) -> usize {
        if self == Self::Byte { 1 } else { 2 }
    }

    /// `ch` (ASCII) as one unit of the file.
    fn encode(self, ch: u8) -> [u8; 2] {
        match self {
            Self::Byte | Self::Utf16Le => [ch, 0],
            Self::Utf16Be => [0, ch],
        }
    }

    /// Where the first line break of `bytes` starts, on a unit boundary.
    fn find(self, bytes: &[u8]) -> Option<usize> {
        if self == Self::Byte {
            return bytes.iter().position(|byte| *byte == b'\n');
        }
        let newline = self.encode(b'\n');
        bytes
            .as_chunks::<2>()
            .0
            .iter()
            .position(|unit| *unit == newline)
            .map(|index| index * 2)
    }
}

pub(crate) struct Reader<'a, S: Source> {
    src: &'a mut S,
    size: u64,
    chunk: u32,
    newline: Newline,
    buf: Vec<u8>,
    /// The source offset of `buf[0]`.
    buf_start: u64,
    pos: usize,
}

impl<'a, S: Source> Reader<'a, S> {
    pub(crate) fn new(src: &'a mut S, offset: u64, newline: Newline, chunk: u32) -> Self {
        let size = src.size();
        Self {
            src,
            size,
            chunk,
            newline,
            buf: Vec::new(),
            buf_start: offset,
            pos: 0,
        }
    }

    /// Starts on `head`, the file's first bytes, already read: a reader made at an offset within
    /// them takes the rest from there instead of reading them again.
    pub(crate) fn with_head(mut self, head: Vec<u8>) -> Self {
        if let Ok(pos) = usize::try_from(self.buf_start)
            && pos <= head.len()
        {
            self.buf = head;
            self.buf_start = 0;
            self.pos = pos;
        }
        self
    }

    /// The offset of the next byte.
    pub(crate) fn offset(&self) -> u64 {
        self.buf_start + self.pos as u64
    }

    /// Reads the next window behind the bytes not yet taken; false at the end of the source.
    fn refill(&mut self) -> Result<bool, Failure> {
        let at = self.buf_start + self.buf.len() as u64;
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
        self.buf.drain(..self.pos);
        self.buf_start += self.pos as u64;
        self.pos = 0;
        self.buf.extend_from_slice(&bytes);
        Ok(true)
    }

    /// Reads the next line into `line`: without its line break (and a `\r` before it), at most
    /// `cap` bytes of it; the rest of a longer line is read past. `None` at the end of the source,
    /// else whether the line was longer than `cap` - its `\r` then is not taken off.
    pub(crate) fn read_line(
        &mut self,
        cap: usize,
        line: &mut Vec<u8>,
    ) -> Result<Option<bool>, Failure> {
        line.clear();
        if self.pos >= self.buf.len() && !self.refill()? {
            return Ok(None);
        }
        let unit = self.newline.unit();
        let mut length = 0usize;
        loop {
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            let (take, newline) = match self.newline.find(rest) {
                Some(at) => (at, true),
                // Whole units only: half of one waits for the next window.
                None => (rest.len() - rest.len() % unit, false),
            };
            let room = cap.saturating_sub(line.len());
            line.extend_from_slice(rest.get(..take.min(room)).unwrap_or_default());
            length = length.saturating_add(take);
            self.pos += take + if newline { unit } else { 0 };
            if newline {
                break;
            }
            if !self.refill()? {
                // The end of the file: an odd byte left over belongs to the last line.
                let rest = self.buf.get(self.pos..).unwrap_or_default();
                let room = cap.saturating_sub(line.len());
                line.extend_from_slice(rest.get(..rest.len().min(room)).unwrap_or_default());
                length = length.saturating_add(rest.len());
                self.pos = self.buf.len();
                break;
            }
        }
        let capped = length > cap;
        let carriage = self.newline.encode(b'\r');
        let carriage = carriage.get(..unit).unwrap_or_default();
        if !capped && line.ends_with(carriage) {
            line.truncate(line.len() - unit);
        }
        Ok(Some(capped))
    }
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
