//! `std::io::Read` and `Seek` over a [`Source`], so the zip reader reads the file through
//! `read-at`: in windows of [`WINDOW`] bytes, `should-stop` asked before each. A failure of the
//! host - or one of the plugin's own limits, raised inside a reader - travels through `std::io`
//! as [`Carried`] and comes out again with [`failure_of_io`], whatever wraps the reader between.

use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

use crate::limits::code;
use crate::types::{Failure, FailureKind, Source};

/// The bytes one `read-at` asks for: the central directory and an XML part's packed bytes come
/// in a few windows, and an image or video part is never touched.
pub(crate) const WINDOW: u32 = 256 << 10;

/// A [`Failure`] carried inside an `io::Error`.
#[derive(Debug)]
pub(crate) struct Carried(pub(crate) Failure);

impl fmt::Display for Carried {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.0.kind)
    }
}

impl std::error::Error for Carried {}

/// `failure` as an `io::Error`.
pub(crate) fn carry(failure: Failure) -> io::Error {
    io::Error::other(Carried(failure))
}

/// The failure `error` carries; any other error of the zip or XML reader is a damaged part.
pub(crate) fn failure_of_io(error: &io::Error) -> Failure {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<Carried>())
        .map_or_else(damaged, |carried| carried.0.clone())
}

/// The failure of a package that cannot be read.
pub(crate) fn damaged() -> Failure {
    Failure::coded(FailureKind::BadData, code::DAMAGED)
}

/// A source read as a stream, one window kept.
pub(crate) struct SourceIo<S: Source> {
    src: S,
    size: u64,
    pos: u64,
    buf: Vec<u8>,
    buf_start: u64,
}

impl<S: Source> SourceIo<S> {
    pub(crate) fn new(src: S) -> Self {
        let size = src.size();
        Self {
            src,
            size,
            pos: 0,
            buf: Vec::new(),
            buf_start: 0,
        }
    }

    /// Reads the window at the position.
    fn refill(&mut self) -> io::Result<()> {
        if self.src.should_stop() {
            return Err(carry(Failure::of(FailureKind::Cancelled)));
        }
        let length = u32::try_from((self.size - self.pos).min(u64::from(WINDOW))).unwrap_or(WINDOW);
        let mut bytes = self.src.read_at(self.pos, length).map_err(carry)?;
        bytes.truncate(length as usize);
        if bytes.is_empty() {
            // A source shorter than announced ends here rather than looping.
            self.size = self.pos;
        }
        self.src
            .progress(self.pos + bytes.len() as u64, Some(self.size));
        self.buf = bytes;
        self.buf_start = self.pos;
        Ok(())
    }
}

impl<S: Source> Read for SourceIo<S> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.pos >= self.size {
            return Ok(0);
        }
        let end = self.buf_start + self.buf.len() as u64;
        if !(self.buf_start..end).contains(&self.pos) {
            self.refill()?;
        }
        let at = usize::try_from(self.pos - self.buf_start).unwrap_or(usize::MAX);
        let window = self.buf.get(at..).unwrap_or_default();
        let count = window.len().min(out.len());
        if let (Some(to), Some(from)) = (out.get_mut(..count), window.get(..count)) {
            to.copy_from_slice(from);
        }
        self.pos += count as u64;
        Ok(count)
    }
}

impl<S: Source> Seek for SourceIo<S> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::End(delta) => self.size.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
        };
        let target = target
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "seek before the start"))?;
        self.pos = target;
        Ok(target)
    }
}

#[cfg(test)]
#[path = "source_io_tests.rs"]
mod tests;
