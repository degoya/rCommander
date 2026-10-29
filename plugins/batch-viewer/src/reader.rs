//! A buffered reader over a [`Source`]: windows of `chunk` bytes through `read-at`, so a call
//! never holds more of the file than one window and one line, and `should-stop` is asked before
//! each. The JSON viewer's reader, reading lines only, and checking UTF-8 on the way when asked.

use crate::encoding::Utf8Check;
use crate::types::{Failure, FailureKind, Source};

pub(crate) struct Reader<'a, S: Source> {
    src: &'a mut S,
    size: u64,
    chunk: u32,
    buf: Vec<u8>,
    buf_start: u64,
    pos: usize,
    /// Fed every window read, for `auto`.
    check: Option<Utf8Check>,
}

impl<'a, S: Source> Reader<'a, S> {
    pub(crate) fn new(src: &'a mut S, offset: u64, chunk: u32) -> Self {
        let size = src.size();
        Self {
            src,
            size,
            chunk,
            buf: Vec::new(),
            buf_start: offset,
            pos: 0,
            check: None,
        }
    }

    /// Checks whether the bytes read are UTF-8 ([`Reader::utf8_valid`]).
    pub(crate) fn with_check(mut self, check: Utf8Check) -> Self {
        self.check = Some(check);
        self
    }

    /// Whether everything read was valid UTF-8; false when nothing was checked.
    pub(crate) fn utf8_valid(&self) -> bool {
        self.check.as_ref().is_some_and(Utf8Check::valid)
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
        if let Some(check) = self.check.as_mut() {
            check.feed(&bytes);
        }
        self.buf = bytes;
        self.buf_start = at;
        self.pos = 0;
        Ok(true)
    }

    /// Reads the next line into `line`: without its `\n` (and a `\r` before it), at most `cap`
    /// bytes of it; the rest of a longer line is read past. `None` at the end of the source,
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
        let mut length = 0usize;
        loop {
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            let (take, newline) = match rest.iter().position(|byte| *byte == b'\n') {
                Some(at) => (at, true),
                None => (rest.len(), false),
            };
            let room = cap.saturating_sub(line.len());
            line.extend_from_slice(rest.get(..take.min(room)).unwrap_or_default());
            length = length.saturating_add(take);
            self.pos += take + usize::from(newline);
            if newline || (self.pos >= self.buf.len() && !self.refill()?) {
                break;
            }
        }
        let capped = length > cap;
        if !capped && line.last() == Some(&b'\r') {
            line.pop();
        }
        Ok(Some(capped))
    }
}
