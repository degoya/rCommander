//! A buffered, seekable reader over a [`Source`]: windows through `read-at`, `should-stop` asked
//! before each, and a **budget** of bytes it may fetch in all, so no reader built on it - ours or a
//! crate's - can walk a file of many GiB. It is `Read + Seek + BufRead` for lofty and
//! kamadak-exif; a failure of the source (or the budget) is kept and handed back by
//! [`SourceReader::take_failure`], since those crates see only an `io::Error`.

use std::io::{self, BufRead, Read, Seek, SeekFrom};

use crate::codes;
use crate::types::{Failure, FailureKind, Source};

/// The most one `read-at` moves (`docs/plugins.md`, "Shared interfaces").
pub(crate) const MAX_READ: usize = 1 << 20;

pub(crate) struct SourceReader<'a, S: Source> {
    src: &'a mut S,
    size: u64,
    pos: u64,
    window: Vec<u8>,
    window_start: u64,
    window_len: u32,
    budget: u64,
    failure: Option<Failure>,
}

impl<'a, S: Source> SourceReader<'a, S> {
    /// A reader of `src` that fetches windows of `window_len` bytes and at most `budget` bytes
    /// in all.
    pub(crate) fn new(src: &'a mut S, window_len: u32, budget: u64) -> Self {
        let size = src.size();
        Self {
            src,
            size,
            pos: 0,
            window: Vec::new(),
            window_start: 0,
            window_len: window_len.clamp(16, 1 << 20),
            budget,
            failure: None,
        }
    }

    /// The file's size as `info` announced it.
    pub(crate) fn size(&self) -> u64 {
        self.size
    }

    /// The failure that ended the last read, if the source or the budget ended it.
    pub(crate) fn take_failure(&mut self) -> Option<Failure> {
        self.failure.take()
    }

    /// One `read-at` of at most `length` bytes at `at`, within the budget.
    fn fetch(&mut self, at: u64, length: usize) -> Result<Vec<u8>, Failure> {
        if self.src.should_stop() {
            return Err(Failure::of(FailureKind::Cancelled));
        }
        let wanted = length.min(MAX_READ);
        let cost = wanted as u64;
        if cost > self.budget {
            return Err(Failure::bad(codes::TOO_LARGE));
        }
        self.budget -= cost;
        let length = u32::try_from(wanted).unwrap_or(u32::MAX);
        let mut bytes = self.src.read_at(at, length)?;
        bytes.truncate(wanted);
        Ok(bytes)
    }

    /// Makes the window cover `at`; false at or past the end of the file.
    fn fill(&mut self, at: u64) -> Result<bool, Failure> {
        let end = self.window_start + self.window.len() as u64;
        if at >= self.window_start && at < end {
            return Ok(true);
        }
        if at >= self.size {
            return Ok(false);
        }
        let length = (self.size - at).min(u64::from(self.window_len));
        let bytes = self.fetch(at, usize::try_from(length).unwrap_or(MAX_READ))?;
        if bytes.is_empty() {
            // A source shorter than announced ends here rather than looping.
            self.size = at;
            return Ok(false);
        }
        self.window = bytes;
        self.window_start = at;
        Ok(true)
    }

    /// Up to `out.len()` bytes at `at` into `out`; fewer only at the end of the file.
    pub(crate) fn read_into(&mut self, at: u64, out: &mut [u8]) -> Result<usize, Failure> {
        let mut done = 0;
        while done < out.len() {
            let Some(here) = at.checked_add(done as u64) else {
                break;
            };
            if !self.fill(here)? {
                break;
            }
            let from = usize::try_from(here - self.window_start).unwrap_or(usize::MAX);
            let available = self.window.get(from..).unwrap_or_default();
            let take = available.len().min(out.len() - done);
            if take == 0 {
                break;
            }
            if let (Some(target), Some(piece)) =
                (out.get_mut(done..done + take), available.get(..take))
            {
                target.copy_from_slice(piece);
            }
            done += take;
        }
        Ok(done)
    }

    /// Exactly `len` bytes at `at`, read straight from the source in windows of at most 1 MiB;
    /// a file that ends first is `bad-data` with `code`.
    pub(crate) fn exact(&mut self, at: u64, len: u64, code: &str) -> Result<Vec<u8>, Failure> {
        let end = at.checked_add(len).ok_or_else(|| Failure::bad(code))?;
        if end > self.size {
            return Err(Failure::bad(code));
        }
        let mut out = Vec::new();
        out.try_reserve_exact(usize::try_from(len).map_err(|_| Failure::bad(codes::TOO_LARGE))?)
            .map_err(|_| Failure::bad(codes::TOO_LARGE))?;
        let mut here = at;
        while here < end {
            let chunk = usize::try_from((end - here).min(MAX_READ as u64)).unwrap_or(MAX_READ);
            let bytes = self.fetch(here, chunk)?;
            if bytes.is_empty() {
                return Err(Failure::bad(code));
            }
            here += bytes.len() as u64;
            out.extend_from_slice(&bytes);
        }
        Ok(out)
    }

    /// At most `len` bytes at `at` through the window: the head of a box or element.
    pub(crate) fn head(&mut self, at: u64, len: usize) -> Result<Vec<u8>, Failure> {
        let mut out = vec![0; len.min(MAX_READ)];
        let got = self.read_into(at, &mut out)?;
        out.truncate(got);
        Ok(out)
    }

    /// Keeps `failure` for [`Self::take_failure`] and hands the crates an `io::Error`.
    fn fail(&mut self, failure: Failure) -> io::Error {
        self.failure = Some(failure);
        io::Error::other("the source failed")
    }

    /// Makes the window cover the position, as `BufRead` wants it.
    fn fill_at_pos(&mut self) -> io::Result<bool> {
        match self.fill(self.pos) {
            Ok(filled) => Ok(filled),
            Err(failure) => Err(self.fail(failure)),
        }
    }
}

impl<S: Source> Read for SourceReader<'_, S> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        match self.read_into(self.pos, out) {
            Ok(got) => {
                self.pos += got as u64;
                Ok(got)
            }
            Err(failure) => Err(self.fail(failure)),
        }
    }
}

impl<S: Source> BufRead for SourceReader<'_, S> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        if !self.fill_at_pos()? {
            return Ok(&[]);
        }
        let from = usize::try_from(self.pos - self.window_start).unwrap_or(usize::MAX);
        Ok(self.window.get(from..).unwrap_or_default())
    }

    fn consume(&mut self, amount: usize) {
        self.pos = self.pos.saturating_add(amount as u64);
    }
}

impl<S: Source> Seek for SourceReader<'_, S> {
    fn seek(&mut self, to: SeekFrom) -> io::Result<u64> {
        let target = match to {
            SeekFrom::Start(offset) => Some(offset),
            SeekFrom::End(delta) => self.size.checked_add_signed(delta),
            SeekFrom::Current(delta) => self.pos.checked_add_signed(delta),
        };
        match target {
            Some(position) => {
                self.pos = position;
                Ok(position)
            }
            None => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek to a negative or overflowing position",
            )),
        }
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        Ok(self.pos)
    }
}

/// A big-endian unsigned number of `bytes` (at most 8).
pub(crate) fn be_uint(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .take(8)
        .fold(0, |value, byte| (value << 8) | u64::from(*byte))
}

/// The big-endian `u32` at `at` of `bytes`, if it is all there.
pub(crate) fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let piece = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([piece[0], piece[1], piece[2], piece[3]]))
}

/// The big-endian `u16` at `at` of `bytes`, if it is all there.
pub(crate) fn be_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let piece = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([piece[0], piece[1]]))
}

/// The big-endian `u64` at `at` of `bytes`, if it is all there.
pub(crate) fn be_u64(bytes: &[u8], at: usize) -> Option<u64> {
    let piece = bytes.get(at..at.checked_add(8)?)?;
    Some(be_uint(piece))
}

#[cfg(test)]
#[path = "io_tests.rs"]
mod tests;
