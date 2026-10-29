//! A buffered, seekable reader over a [`Source`]: windows of `chunk` bytes through `read-at`, so
//! a call never holds more of the file than one window, and `should-stop` is asked before each.
//! The JSON viewer's reader, without its line reads: the YAML lexer reads the lines itself.

use crate::types::{Failure, FailureKind, Source};

/// The byte stream the lexer reads: a reader over a source, or text in memory.
pub(crate) trait Bytes {
    /// The next byte, without taking it; `None` at the end.
    fn peek(&mut self) -> Result<Option<u8>, Failure>;
    /// Takes the byte the last `peek` returned.
    fn bump(&mut self);
    /// The offset of the next byte.
    fn offset(&self) -> u64;
    /// Moves to `offset`.
    fn seek(&mut self, offset: u64);
}

/// The bytes of the previous window a new one keeps in front.
const KEEP_BYTES: usize = 8;

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

    pub(crate) fn source(&mut self) -> &mut S {
        &mut *self.src
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
        // The last bytes of the window before stay, so the lexer's look-ahead of a few bytes
        // goes back without reading the window again.
        let kept = self.buf.len().min(KEEP_BYTES);
        let mut window = Vec::with_capacity(kept + bytes.len());
        window.extend_from_slice(self.buf.get(self.buf.len() - kept..).unwrap_or_default());
        window.extend_from_slice(&bytes);
        self.buf = window;
        self.buf_start = at - kept as u64;
        self.pos = kept;
        Ok(true)
    }

    /// Hands the bytes `start..end` to `out`, read straight from the source in windows.
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
        }
        Ok(())
    }
}

impl<S: Source> Bytes for Reader<'_, S> {
    fn peek(&mut self) -> Result<Option<u8>, Failure> {
        if let Some(byte) = self.buf.get(self.pos) {
            return Ok(Some(*byte));
        }
        if self.refill()? {
            Ok(self.buf.get(self.pos).copied())
        } else {
            Ok(None)
        }
    }

    fn bump(&mut self) {
        self.pos += 1;
    }

    fn offset(&self) -> u64 {
        self.buf_start + self.pos as u64
    }

    /// Keeps the window when `offset` is inside it.
    fn seek(&mut self, offset: u64) {
        let end = self.buf_start + self.buf.len() as u64;
        if (self.buf_start..=end).contains(&offset) {
            self.pos = usize::try_from(offset - self.buf_start).unwrap_or(self.buf.len());
        } else {
            self.buf.clear();
            self.buf_start = offset;
            self.pos = 0;
        }
    }
}

/// Text in memory as a byte stream, for the tests.
#[cfg(test)]
pub(crate) struct SliceBytes<'a> {
    bytes: &'a [u8],
    pos: usize,
}

#[cfg(test)]
impl<'a> SliceBytes<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }
}

#[cfg(test)]
impl Bytes for SliceBytes<'_> {
    fn peek(&mut self) -> Result<Option<u8>, Failure> {
        Ok(self.bytes.get(self.pos).copied())
    }

    fn bump(&mut self) {
        self.pos += 1;
    }

    fn offset(&self) -> u64 {
        self.pos as u64
    }

    fn seek(&mut self, offset: u64) {
        self.pos = usize::try_from(offset).unwrap_or(self.bytes.len());
    }
}
