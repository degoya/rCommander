//! A buffered reader over a [`Source`]: windows of `chunk` bytes through `read-at`, so a call
//! never holds more of the file than one window and one line, and `should-stop` is asked before
//! each. The batch viewer's reader, splitting lines by the file's code unit: a byte, or a UTF-16
//! unit, so a `\n` of UTF-16 is the two bytes of one unit and never half of another.

use rc_plugin_text::Encoding;

use crate::types::{Failure, FailureKind, Source};

/// How wide a code unit of the file is, and in which byte order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    Byte,
    Utf16Le,
    Utf16Be,
}

impl Unit {
    pub(crate) fn of(encoding: Encoding) -> Self {
        match encoding {
            Encoding::Utf16Le => Self::Utf16Le,
            Encoding::Utf16Be => Self::Utf16Be,
            _ => Self::Byte,
        }
    }

    fn newline(self) -> &'static [u8] {
        match self {
            Self::Byte => b"\n",
            Self::Utf16Le => b"\n\0",
            Self::Utf16Be => b"\0\n",
        }
    }

    fn carriage_return(self) -> &'static [u8] {
        match self {
            Self::Byte => b"\r",
            Self::Utf16Le => b"\r\0",
            Self::Utf16Be => b"\0\r",
        }
    }

    /// Where the first newline of `rest` starts; `rest` starts on a unit.
    fn find_newline(self, rest: &[u8]) -> Option<usize> {
        match self {
            Self::Byte => rest.iter().position(|byte| *byte == b'\n'),
            Self::Utf16Le | Self::Utf16Be => rest
                .as_chunks::<2>()
                .0
                .iter()
                .position(|unit| unit.as_slice() == self.newline())
                .map(|at| at * 2),
        }
    }
}

pub(crate) struct Reader<'a, S: Source> {
    src: &'a mut S,
    size: u64,
    chunk: u32,
    unit: Unit,
    buf: Vec<u8>,
    buf_start: u64,
    pos: usize,
}

impl<'a, S: Source> Reader<'a, S> {
    /// A reader from `offset`, a line start; `chunk` is even, so every window starts on a unit.
    pub(crate) fn new(src: &'a mut S, offset: u64, chunk: u32, unit: Unit) -> Self {
        let size = src.size();
        Self {
            src,
            size,
            chunk,
            unit,
            buf: Vec::new(),
            buf_start: offset,
            pos: 0,
        }
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

    /// Reads the next line's bytes into `line`: without its newline (and a carriage return
    /// before it), at most `cap` bytes of it - `cap` even, so a UTF-16 unit stays whole; the rest
    /// of a longer line is read past. `None` at the end of the source, else whether the line was
    /// longer than `cap` - its carriage return then is not taken off.
    pub(crate) fn read_line(
        &mut self,
        cap: usize,
        line: &mut Vec<u8>,
    ) -> Result<Option<bool>, Failure> {
        line.clear();
        if self.pos >= self.buf.len() && !self.refill()? {
            return Ok(None);
        }
        let newline_len = self.unit.newline().len();
        let mut length = 0usize;
        loop {
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            let (take, newline) = match self.unit.find_newline(rest) {
                Some(at) => (at, true),
                None => (rest.len(), false),
            };
            let room = cap.saturating_sub(line.len());
            line.extend_from_slice(rest.get(..take.min(room)).unwrap_or_default());
            length = length.saturating_add(take);
            self.pos += take + if newline { newline_len } else { 0 };
            if newline || (self.pos >= self.buf.len() && !self.refill()?) {
                break;
            }
        }
        let capped = length > cap;
        let carriage_return = self.unit.carriage_return();
        if !capped && line.ends_with(carriage_return) {
            line.truncate(line.len() - carriage_return.len());
        }
        Ok(Some(capped))
    }
}
