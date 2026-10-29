//! A buffered reader over a [`Source`]: windows of `chunk` bytes through `read-at`, so a call
//! never holds more of the file than one window and the few bytes the lexer looks ahead, and
//! `should-stop` is asked before each. Every byte is read once; looking ahead keeps the bytes
//! not yet taken in front of the next window.

use crate::types::{Failure, FailureKind, Source};

pub(crate) struct Reader<'a, S: Source> {
    src: &'a mut S,
    size: u64,
    chunk: u32,
    buf: Vec<u8>,
    /// The source offset of `buf[0]`.
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

    /// The offset of the next byte.
    pub(crate) fn offset(&self) -> u64 {
        self.buf_start + self.pos as u64
    }

    /// The next byte, without taking it; `None` at the end.
    pub(crate) fn peek(&mut self) -> Result<Option<u8>, Failure> {
        if let Some(byte) = self.buf.get(self.pos) {
            return Ok(Some(*byte));
        }
        Ok(self.look(1)?.first().copied())
    }

    /// The byte `ahead` bytes after the next one, without taking anything.
    pub(crate) fn peek_at(&mut self, ahead: usize) -> Result<Option<u8>, Failure> {
        if let Some(byte) = self.buf.get(self.pos + ahead) {
            return Ok(Some(*byte));
        }
        Ok(self.look(ahead + 1)?.get(ahead).copied())
    }

    /// The next `n` bytes, without taking them; fewer only at the end of the source.
    pub(crate) fn look(&mut self, n: usize) -> Result<&[u8], Failure> {
        while self.buf.len() - self.pos < n && self.refill()? {}
        let end = self.buf.len().min(self.pos + n);
        Ok(self.buf.get(self.pos..end).unwrap_or_default())
    }

    /// Takes `n` bytes a `look` or `peek` returned.
    pub(crate) fn bump(&mut self, n: usize) {
        self.pos = (self.pos + n).min(self.buf.len());
    }

    /// Moves to `offset`; keeps the window when `offset` is inside it.
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
}
