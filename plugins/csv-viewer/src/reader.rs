//! A buffered, seekable reader over a [`Source`]: windows of `chunk` bytes through `read-at`, so
//! a call never holds more of the file than one window, and `should-stop` is asked before each.
//! The JSON viewer's reader, without what CSV does not need.

use crate::types::{Failure, FailureKind, Source};

pub(crate) struct Reader<'a, S: Source> {
    src: &'a mut S,
    size: u64,
    chunk: u32,
    buf: Vec<u8>,
    buf_start: u64,
    pos: usize,
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
        }
    }

    /// Moves to `offset`, keeping the window when it is inside.
    pub(crate) fn seek(&mut self, offset: u64) {
        let end = self.buf_start + self.buf.len() as u64;
        if (self.buf_start..=end).contains(&offset) {
            self.pos = usize::try_from(offset - self.buf_start).unwrap_or(self.buf.len());
        } else {
            self.buf.clear();
            self.buf_start = offset;
            self.pos = 0;
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

    /// The next byte, without taking it; `None` at the end.
    pub(crate) fn peek(&mut self) -> Result<Option<u8>, Failure> {
        if let Some(byte) = self.buf.get(self.pos) {
            return Ok(Some(*byte));
        }
        if self.refill()? {
            Ok(self.buf.get(self.pos).copied())
        } else {
            Ok(None)
        }
    }

    /// Takes the byte the last `peek` returned.
    pub(crate) fn bump(&mut self) {
        self.pos += 1;
    }

    /// Hands the bytes up to the next `\n` to `out`, in pieces and without the `\n`, and moves
    /// past it; false when the source had no byte left.
    pub(crate) fn feed_line(&mut self, out: &mut dyn FnMut(&[u8])) -> Result<bool, Failure> {
        let mut any = false;
        loop {
            if self.pos >= self.buf.len() && !self.refill()? {
                return Ok(any);
            }
            any = true;
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            match rest.iter().position(|byte| *byte == b'\n') {
                Some(at) => {
                    out(rest.get(..at).unwrap_or_default());
                    self.pos += at + 1;
                    return Ok(true);
                }
                None => {
                    out(rest);
                    self.pos = self.buf.len();
                }
            }
        }
    }

    /// The next line without its `\n` (and without a `\r` before it), at most `cap` bytes of it.
    /// `None` at the end of the source. A line longer than `cap` is returned `capped` and left
    /// unread after the cap.
    pub(crate) fn read_line(&mut self, cap: usize) -> Result<Option<Line>, Failure> {
        if self.peek()?.is_none() {
            return Ok(None);
        }
        let mut bytes = Vec::new();
        loop {
            if self.pos >= self.buf.len() && !self.refill()? {
                break;
            }
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            let room = cap - bytes.len();
            let (take, newline) = match rest.iter().position(|byte| *byte == b'\n') {
                Some(at) if at <= room => (at, true),
                _ => (rest.len().min(room), false),
            };
            bytes.extend_from_slice(rest.get(..take).unwrap_or_default());
            self.pos += take;
            if newline {
                self.pos += 1;
                break;
            }
            if bytes.len() == cap {
                return Ok(Some(Line {
                    bytes,
                    capped: true,
                }));
            }
        }
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
        Ok(Some(Line {
            bytes,
            capped: false,
        }))
    }

    /// Hands the bytes `start..end` to `out` in windows, read straight from the source.
    pub(crate) fn copy_range(
        &mut self,
        start: u64,
        end: u64,
        out: &mut dyn FnMut(&[u8]) -> Result<(), Failure>,
    ) -> Result<(), Failure> {
        let mut at = start;
        while at < end {
            if self.src.should_stop() {
                return Err(Failure::of(FailureKind::Cancelled));
            }
            let length = u32::try_from((end - at).min(u64::from(self.chunk))).unwrap_or(self.chunk);
            let bytes = self.src.read_at(at, length)?;
            if bytes.is_empty() {
                break;
            }
            let bytes = bytes.get(..length as usize).unwrap_or(&bytes);
            out(bytes)?;
            at += bytes.len() as u64;
            self.src.progress(at, Some(end));
        }
        Ok(())
    }
}

/// One line read by [`Reader::read_line`].
#[derive(Debug)]
pub(crate) struct Line {
    pub(crate) bytes: Vec<u8>,
    /// Longer than the cap: the bytes are its beginning.
    pub(crate) capped: bool,
}
