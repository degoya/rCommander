//! A line reader over a [`Source`]: windows of `chunk` bytes through `read-at`, so a call never
//! holds more of the file than one window and the head of one line, and `should-stop` is asked
//! before each window. A diff is classified line by line, by how a line starts, so a line is
//! handed out as its first bytes only; the rest of a longer line is skipped, or jumped over.

use crate::types::{Failure, FailureKind, Source};

/// How a line handed out ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum End {
    /// At a `\n`, which is taken and not handed out.
    Newline,
    /// At the end of the source.
    Eof,
    /// The line goes on past the bytes kept: [`LineReader::skip_line`] or a seek follows.
    Cut,
}

pub(crate) struct LineReader<'a, S: Source> {
    src: &'a mut S,
    size: u64,
    chunk: u32,
    buf: Vec<u8>,
    /// The source offset of `buf[0]`.
    buf_start: u64,
    pos: usize,
}

impl<'a, S: Source> LineReader<'a, S> {
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

    /// The next line into `head` - cleared first, its `\n` left out, at most `keep` bytes - and
    /// how it ends; `None` at the end of the source.
    pub(crate) fn next_line(
        &mut self,
        head: &mut Vec<u8>,
        keep: usize,
    ) -> Result<Option<End>, Failure> {
        head.clear();
        let mut any = false;
        loop {
            if self.pos >= self.buf.len() && !self.refill()? {
                return Ok(any.then_some(End::Eof));
            }
            any = true;
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            let newline = rest.iter().position(|byte| *byte == b'\n');
            let end = newline.unwrap_or(rest.len());
            let room = keep.saturating_sub(head.len());
            if end > room {
                head.extend_from_slice(rest.get(..room).unwrap_or_default());
                self.pos += room;
                return Ok(Some(End::Cut));
            }
            head.extend_from_slice(rest.get(..end).unwrap_or_default());
            if newline.is_some() {
                self.pos += end + 1;
                return Ok(Some(End::Newline));
            }
            self.pos += end;
        }
    }

    /// Takes the rest of a line [`Self::next_line`] cut, through its `\n`.
    pub(crate) fn skip_line(&mut self) -> Result<(), Failure> {
        loop {
            if self.pos >= self.buf.len() && !self.refill()? {
                return Ok(());
            }
            let rest = self.buf.get(self.pos..).unwrap_or_default();
            match rest.iter().position(|byte| *byte == b'\n') {
                Some(at) => {
                    self.pos += at + 1;
                    return Ok(());
                }
                None => self.pos = self.buf.len(),
            }
        }
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

    /// Reads the next window, once every byte of the last one is taken; false at the end of the
    /// source.
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
        self.buf_start = at;
        self.buf = bytes;
        self.pos = 0;
        Ok(true)
    }
}

#[cfg(test)]
#[path = "reader_tests.rs"]
mod tests;
