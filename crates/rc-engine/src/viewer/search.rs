//! Search in the viewer: text in the file's encoding, or bytes typed as hex; forward or
//! backward from an offset, streamed over the whole file in chunks, with progress and a cancel
//! flag checked between chunks.
//!
//! A text is turned into the bytes it has in the file ([`Needle::text`]); without case each
//! character may be any of its case forms. Matches in UTF-16 start on a code unit only. Chunks
//! overlap by the longest match less one byte, so a match across a chunk boundary is found.
//! Backwards over a backend that reaches an offset only by reading up to it (an archive entry),
//! the search reads the file once forwards and keeps the last match, instead of reading it from
//! its start again for every chunk (RC-080-46). Over one that starts a read at an offset but
//! cannot seek (FTP's `REST`), where every read is a new data connection, it reads spans before
//! the offset forwards on one stream each, a span twice the one before up to
//! `MAX_SPAN_CHUNKS` chunks, instead of a connection per chunk (RC-080-57).

use std::sync::atomic::{AtomicBool, Ordering};

use rc_vfs::VfsError;
use serde::Serialize;
use tokio::io::AsyncReadExt;

use super::{Detected, FileWindow, encoding::encode_char};

/// How much one step of a search reads.
pub const SEARCH_CHUNK: usize = 1024 * 1024;

/// The longest span, in chunks, a backward search reads on one stream over a backend where every
/// read is a new connection; its memory stays one chunk and the overlap whatever the span.
const MAX_SPAN_CHUNKS: u64 = 32;

/// Which way a search goes from its offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// The first match starting at or after the offset.
    Forward,
    /// The last match starting before the offset.
    Backward,
}

/// Why a search text or hex string cannot be searched for.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PatternError {
    #[error("the search text is empty")]
    Empty,
    #[error("{token:?} is not a sequence of hex bytes")]
    InvalidHex { token: String },
    #[error("the search text has characters that {encoding} cannot hold")]
    NotEncodable { encoding: &'static str },
}

impl PatternError {
    /// Every code [`PatternError::code`] returns, for the i18n catalogue.
    pub const ALL_CODES: &[&str] = &["search_empty", "search_invalid_hex", "search_not_encodable"];

    /// The stable code; the message is `error.<code>` in the catalogue.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "search_empty",
            Self::InvalidHex { .. } => "search_invalid_hex",
            Self::NotEncodable { .. } => "search_not_encodable",
        }
    }
}

/// Hex bytes as a person types them: `4F 6B 0a`, `4f6b0a`, `0x4F,0x6B`.
///
/// # Errors
///
/// [`PatternError::Empty`] for no bytes, [`PatternError::InvalidHex`] for a token that is not
/// an even number of hex digits.
pub fn parse_hex(text: &str) -> Result<Vec<u8>, PatternError> {
    let mut bytes = Vec::new();
    for token in text.split(|c: char| c.is_whitespace() || c == ',') {
        if token.is_empty() {
            continue;
        }
        let digits = token
            .strip_prefix("0x")
            .or_else(|| token.strip_prefix("0X"))
            .unwrap_or(token);
        let invalid = || PatternError::InvalidHex {
            token: token.to_owned(),
        };
        if digits.is_empty() || digits.len() % 2 == 1 {
            return Err(invalid());
        }
        for pair in digits.as_bytes().chunks(2) {
            let pair = std::str::from_utf8(pair).map_err(|_| invalid())?;
            bytes.push(u8::from_str_radix(pair, 16).map_err(|_| invalid())?);
        }
    }
    if bytes.is_empty() {
        return Err(PatternError::Empty);
    }
    Ok(bytes)
}

/// What a search looks for, as bytes of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Needle {
    /// Per character (or byte), the byte strings it may be in the file.
    units: Vec<Vec<Vec<u8>>>,
    /// The bytes a match can start with.
    first: Vec<bool>,
    max_len: usize,
    /// Matches start where `(offset - base) % align == 0`.
    align: u64,
    base: u64,
}

impl Needle {
    /// Exactly these bytes, at any offset.
    ///
    /// # Errors
    ///
    /// [`PatternError::Empty`].
    pub fn bytes(bytes: &[u8]) -> Result<Self, PatternError> {
        Self::build(bytes.iter().map(|&byte| vec![vec![byte]]).collect(), 1, 0)
    }

    /// `text` as a file in `detected`'s encoding holds it; without `case_sensitive` each
    /// character may also be its upper or lower case form.
    ///
    /// # Errors
    ///
    /// [`PatternError::Empty`], or [`PatternError::NotEncodable`] for a character the encoding
    /// has no bytes for.
    pub fn text(
        text: &str,
        case_sensitive: bool,
        detected: Detected,
    ) -> Result<Self, PatternError> {
        let encoding = detected.encoding;
        let mut units = Vec::new();
        for c in text.chars() {
            let mut forms = vec![c];
            if !case_sensitive {
                for other in [single(c.to_lowercase()), single(c.to_uppercase())]
                    .into_iter()
                    .flatten()
                {
                    if !forms.contains(&other) {
                        forms.push(other);
                    }
                }
            }
            let alternatives: Vec<Vec<u8>> = forms
                .into_iter()
                .filter_map(|form| encode_char(form, encoding))
                .collect();
            if alternatives.is_empty() {
                return Err(PatternError::NotEncodable {
                    encoding: encoding.label(),
                });
            }
            units.push(alternatives);
        }
        Self::build(units, encoding.unit() as u64, detected.bom_len)
    }

    fn build(units: Vec<Vec<Vec<u8>>>, align: u64, base: u64) -> Result<Self, PatternError> {
        let Some(first_unit) = units.first() else {
            return Err(PatternError::Empty);
        };
        let mut first = vec![false; 256];
        for alternative in first_unit {
            if let Some(&byte) = alternative.first() {
                first[usize::from(byte)] = true;
            }
        }
        let max_len = units
            .iter()
            .map(|alternatives| alternatives.iter().map(Vec::len).max().unwrap_or(0))
            .sum();
        Ok(Self {
            units,
            first,
            max_len,
            align,
            base,
        })
    }

    /// The longest a match can be, in bytes.
    #[must_use]
    pub fn max_len(&self) -> usize {
        self.max_len
    }

    fn aligned(&self, offset: u64) -> bool {
        offset >= self.base && (offset - self.base).is_multiple_of(self.align)
    }

    fn match_len(&self, hay: &[u8], at: usize) -> Option<usize> {
        let mut pos = at;
        for alternatives in &self.units {
            let found = alternatives.iter().find(|alternative| {
                hay.get(pos..pos + alternative.len()) == Some(alternative.as_slice())
            })?;
            pos += found.len();
        }
        Some(pos - at)
    }

    /// The first match lying wholly in `hay` (which is at `base` in the file): its index and
    /// length.
    pub(crate) fn find(&self, hay: &[u8], base: u64) -> Option<(usize, usize)> {
        let mut at = 0;
        while at < hay.len() {
            at += hay[at..]
                .iter()
                .position(|&byte| self.first[usize::from(byte)])?;
            if self.aligned(base + at as u64)
                && let Some(len) = self.match_len(hay, at)
            {
                return Some((at, len));
            }
            at += 1;
        }
        None
    }

    /// The last match lying wholly in `hay` (at `base`) that starts before index `before`.
    pub(crate) fn rfind(&self, hay: &[u8], base: u64, before: usize) -> Option<(usize, usize)> {
        let mut end = before.min(hay.len());
        while end > 0 {
            let at = hay[..end]
                .iter()
                .rposition(|&byte| self.first[usize::from(byte)])?;
            if self.aligned(base + at as u64)
                && let Some(len) = self.match_len(hay, at)
            {
                return Some((at, len));
            }
            end = at;
        }
        None
    }
}

/// The character `forms` holds when it is exactly one.
fn single(mut forms: impl Iterator<Item = char>) -> Option<char> {
    let first = forms.next()?;
    forms.next().is_none().then_some(first)
}

/// Where a match is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Match {
    pub offset: u64,
    pub len: u64,
}

/// How a search ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchOutcome {
    Found(Match),
    NotFound,
    /// The cancel flag was set.
    Cancelled,
}

/// Searches `window`'s file for `needle` from `from` in `direction`. `progress` hears how many
/// bytes have been searched after every chunk; setting `cancel` stops the search before the
/// next chunk.
///
/// # Errors
///
/// The backend failed to read.
pub async fn search(
    window: &FileWindow,
    needle: &Needle,
    from: u64,
    direction: Direction,
    cancel: &AtomicBool,
    progress: &mut (dyn FnMut(u64) + Send),
) -> Result<SearchOutcome, VfsError> {
    search_chunked(
        window,
        needle,
        from,
        direction,
        cancel,
        progress,
        SEARCH_CHUNK,
    )
    .await
}

/// [`search`] in chunks of `chunk` bytes; tests use small ones to cross many boundaries.
pub(crate) async fn search_chunked(
    window: &FileWindow,
    needle: &Needle,
    from: u64,
    direction: Direction,
    cancel: &AtomicBool,
    progress: &mut (dyn FnMut(u64) + Send),
    chunk: usize,
) -> Result<SearchOutcome, VfsError> {
    let chunk = chunk.max(needle.max_len()).max(1);
    let keep = needle.max_len().saturating_sub(1);
    let size = window.size();
    match direction {
        Direction::Forward => {
            if from >= size {
                return Ok(SearchOutcome::NotFound);
            }
            let mut stream = window.stream_from(from).await?.take(size - from);
            let mut buf = Vec::with_capacity(chunk + keep);
            let mut base = from;
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(SearchOutcome::Cancelled);
                }
                let before = buf.len();
                (&mut stream)
                    .take(chunk as u64)
                    .read_to_end(&mut buf)
                    .await
                    .map_err(|error| VfsError::from_io(error, window.path()))?;
                if let Some((at, len)) = needle.find(&buf, base) {
                    return Ok(found(base + at as u64, len));
                }
                progress(base + buf.len() as u64 - from);
                if buf.len() - before < chunk {
                    return Ok(SearchOutcome::NotFound);
                }
                let consumed = buf.len().saturating_sub(keep);
                buf.drain(..consumed);
                base += consumed as u64;
            }
        }
        Direction::Backward => {
            let before = from.min(size);
            let mut limit = before;
            let pass = Pass {
                chunk,
                keep,
                cancel,
            };
            let mut span = chunk as u64;
            while limit > 0 {
                if cancel.load(Ordering::Relaxed) {
                    return Ok(SearchOutcome::Cancelled);
                }
                let searched = before - limit;
                if window.skips_to_offsets() {
                    let mut onwards = |done: u64| progress(searched + done.min(limit));
                    return pass
                        .last_before(window, needle, 0, limit, &mut onwards)
                        .await;
                }
                if window.resumes_at_offsets() {
                    span = span.saturating_mul(2).min(chunk as u64 * MAX_SPAN_CHUNKS);
                    let start = limit.saturating_sub(span);
                    let room = limit - start;
                    let mut onwards = |done: u64| progress(searched + done.min(room));
                    match pass
                        .last_before(window, needle, start, limit, &mut onwards)
                        .await?
                    {
                        SearchOutcome::NotFound => limit = start,
                        outcome => return Ok(outcome),
                    }
                    continue;
                }
                let start = limit.saturating_sub(chunk as u64);
                let end = (limit + keep as u64).min(size);
                let len = usize::try_from(end - start).unwrap_or(usize::MAX);
                let bytes = window.read_uncached(start, len).await?;
                let first_after = usize::try_from(limit - start).unwrap_or(usize::MAX);
                if let Some((at, len)) = needle.rfind(&bytes, start, first_after) {
                    return Ok(found(start + at as u64, len));
                }
                progress(before - start);
                limit = start;
            }
            Ok(SearchOutcome::NotFound)
        }
    }
}

/// One pass forwards over a part of the file, in chunks.
struct Pass<'a> {
    chunk: usize,
    keep: usize,
    cancel: &'a AtomicBool,
}

impl Pass<'_> {
    /// The last match starting at or after `start` and before `limit`, read on one stream from
    /// `start`; `progress` hears the bytes read so far.
    async fn last_before(
        &self,
        window: &FileWindow,
        needle: &Needle,
        start: u64,
        limit: u64,
        progress: &mut (dyn FnMut(u64) + Send),
    ) -> Result<SearchOutcome, VfsError> {
        let end = limit.saturating_add(self.keep as u64).min(window.size());
        let mut stream = window
            .stream_from(start)
            .await?
            .take(end.saturating_sub(start));
        let mut buf = Vec::with_capacity(self.chunk + self.keep);
        let mut base = start;
        let mut last = None;
        loop {
            if self.cancel.load(Ordering::Relaxed) {
                return Ok(SearchOutcome::Cancelled);
            }
            let before = buf.len();
            (&mut stream)
                .take(self.chunk as u64)
                .read_to_end(&mut buf)
                .await
                .map_err(|error| VfsError::from_io(error, window.path()))?;
            let first_after = usize::try_from(limit.saturating_sub(base)).unwrap_or(usize::MAX);
            // A later buffer holds every match of the overlap again: its last match is the latest.
            if let Some((at, len)) = needle.rfind(&buf, base, first_after) {
                last = Some((base + at as u64, len));
            }
            progress(base - start + buf.len() as u64);
            if buf.len() - before < self.chunk {
                break;
            }
            let consumed = buf.len().saturating_sub(self.keep);
            buf.drain(..consumed);
            base += consumed as u64;
        }
        Ok(last.map_or(SearchOutcome::NotFound, |(offset, len)| found(offset, len)))
    }
}

fn found(offset: u64, len: usize) -> SearchOutcome {
    SearchOutcome::Found(Match {
        offset,
        len: len as u64,
    })
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
