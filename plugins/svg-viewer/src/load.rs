//! The file into memory - usvg reads a whole document - and an `.svgz` inflated under a limit,
//! since usvg's own inflation has none and a few kilobytes of gzip unpack to gigabytes.

use std::io::Read;
use std::rc::Rc;

use flate2::read::GzDecoder;

use crate::code;
use crate::types::{Failure, FailureKind, Source};

/// The window the file is read through: the most one `read-at` serves.
pub const READ_CHUNK: u32 = 1 << 20;

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

/// Whether `bytes` start as a gzip stream does.
#[must_use]
pub fn is_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&GZIP_MAGIC)
}

/// The whole file, read in windows of [`READ_CHUNK`]; `too_large` beyond `limit`, `cancelled`
/// when the person moved on.
pub fn read_all<S: Source>(src: &mut S, limit: u64) -> Result<Vec<u8>, Failure> {
    let size = src.size();
    if size > limit {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_LARGE));
    }
    let size =
        usize::try_from(size).map_err(|_| Failure::coded(FailureKind::BadData, code::TOO_LARGE))?;
    let mut bytes = Vec::with_capacity(size);
    while bytes.len() < size {
        if src.should_stop() {
            return Err(Failure::of(FailureKind::Cancelled));
        }
        let window = (size - bytes.len()).min(READ_CHUNK as usize);
        let piece = src.read_at(bytes.len() as u64, window as u32)?;
        if piece.is_empty() {
            // The file got shorter since its size was asked: what there is.
            break;
        }
        bytes.extend_from_slice(&piece[..piece.len().min(window)]);
        src.progress(bytes.len() as u64, Some(size as u64));
    }
    Ok(bytes)
}

/// `gzip` inflated, at most `limit` bytes: `too_large` beyond it, `bad_gzip` for a stream that
/// does not inflate. Reading stops one byte past the limit, so a bomb costs no more than that.
pub fn inflate(gzip: &[u8], limit: u64) -> Result<Vec<u8>, Failure> {
    let mut out = Vec::new();
    GzDecoder::new(gzip)
        .take(limit.saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|_| Failure::coded(FailureKind::BadData, code::BAD_GZIP))?;
    if out.len() as u64 > limit {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_LARGE));
    }
    Ok(out)
}

/// The SVG in memory as a source, for the XML viewer that indexes and colours it; shared with
/// the renderer, never copied.
pub struct Memory {
    bytes: Rc<Vec<u8>>,
}

impl Memory {
    #[must_use]
    pub fn new(bytes: Rc<Vec<u8>>) -> Self {
        Self { bytes }
    }
}

impl Source for Memory {
    fn size(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&mut self, offset: u64, length: u32) -> Result<Vec<u8>, Failure> {
        let start = usize::try_from(offset)
            .unwrap_or(usize::MAX)
            .min(self.bytes.len());
        let end = start.saturating_add(length as usize).min(self.bytes.len());
        Ok(self.bytes[start..end].to_vec())
    }
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
