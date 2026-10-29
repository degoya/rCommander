//! Content search: a text or a regular expression in a file's contents, streamed.
//!
//! A file is read in chunks of [`CHUNK`] bytes and decoded once per wanted encoding (UTF-8,
//! UTF-16 LE/BE, Windows-1252, or automatic: detected from the first chunk as the viewer does -
//! a BOM, UTF-16 without one, UTF-8, else Windows-1252), each into a
//! window that keeps the tail of the text before it, so a match across a chunk boundary is
//! found. Memory stays bounded by the chunk and the window, whatever the file's size. A plain
//! text never spans more than its window; a regular expression finds matches up to
//! [`REGEX_OVERLAP`] bytes long across a boundary (longer ones within a chunk).

use encoding_rs::{CoderResult, Decoder, Encoding, UTF_8, UTF_16BE, UTF_16LE, WINDOWS_1252};
use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt as _};

use super::SearchCancel;
use crate::{EngineError, viewer};

/// Bytes read per step: the viewer's head size, so the first chunk is what its detection sees.
pub(crate) const CHUNK: usize = viewer::DETECT_BYTES;
/// How much text before a chunk a regular expression sees again.
pub(crate) const REGEX_OVERLAP: usize = 4096;
/// How far a match touching a chunk's end is held back to see what follows it.
const SUSPECT_LIMIT: usize = 64 * 1024;
/// The bytes the binary check looks at.
const BINARY_PROBE: usize = 8192;

/// How a file's bytes become text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextEncoding {
    /// Detected from the file's first chunk as the viewer detects it.
    Auto,
    Utf8,
    Utf16Le,
    Utf16Be,
    /// Windows-1252, the superset of Latin-1 (ISO-8859-1) every web decoder uses for it.
    Windows1252,
}

impl TextEncoding {
    /// The decoder for a file whose first chunk is `head`.
    fn decoder(self, head: &[u8]) -> Decoder {
        match self {
            Self::Auto => viewer::detect(head)
                .encoding
                .codec()
                // Detection never answers Latin-1, the one encoding without a codec.
                .unwrap_or(WINDOWS_1252)
                .new_decoder_with_bom_removal(),
            Self::Utf8 => UTF_8.new_decoder_with_bom_removal(),
            Self::Utf16Le => UTF_16LE.new_decoder_with_bom_removal(),
            Self::Utf16Be => UTF_16BE.new_decoder_with_bom_removal(),
            Self::Windows1252 => WINDOWS_1252.new_decoder_without_bom_handling(),
        }
    }
}

/// What to look for inside files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct ContentQuery {
    /// The text, or a regular expression when `regex` is set; never empty.
    pub text: String,
    pub regex: bool,
    pub case_sensitive: bool,
    /// Only where the text stands as a whole word.
    pub whole_words: bool,
    /// The encodings to search in, all in one pass; empty means `[auto]`.
    pub encodings: Vec<TextEncoding>,
    /// Hits are the files that do *not* contain it.
    pub not_containing: bool,
    /// Files that look binary (a NUL byte near the start, no UTF-16) are not searched.
    pub skip_binary: bool,
}

/// What a scan of one file found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scan {
    Found,
    Absent,
    /// Skipped as binary.
    Binary,
    Cancelled,
}

/// A compiled [`ContentQuery`].
#[derive(Debug)]
pub(crate) struct ContentMatcher {
    regex: Regex,
    overlap: usize,
    encodings: Vec<TextEncoding>,
    pub(crate) not_containing: bool,
    skip_binary: bool,
}

impl ContentMatcher {
    pub(crate) fn new(query: &ContentQuery) -> Result<Self, EngineError> {
        if query.text.is_empty() {
            return Err(EngineError::InvalidSearch {
                reason: "the text to find is empty".to_owned(),
            });
        }
        let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        let (pattern, overlap) = if query.regex {
            (query.text.clone(), REGEX_OVERLAP)
        } else {
            // One character of the text matches one of at most 4 bytes, case folded or not.
            (
                regex::escape(&query.text),
                query.text.chars().count() * 4 + 4,
            )
        };
        let pattern = if !query.whole_words {
            pattern
        } else if query.regex {
            format!(r"\b(?:{pattern})\b")
        } else {
            let head = if word(query.text.chars().next()) {
                r"\b"
            } else {
                ""
            };
            let tail = if word(query.text.chars().next_back()) {
                r"\b"
            } else {
                ""
            };
            format!("{head}{pattern}{tail}")
        };
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!query.case_sensitive)
            .multi_line(true)
            .build()
            .map_err(|error| EngineError::InvalidSearch {
                reason: error.to_string(),
            })?;
        let encodings = if query.encodings.is_empty() {
            vec![TextEncoding::Auto]
        } else {
            query.encodings.clone()
        };
        Ok(Self {
            regex,
            overlap,
            encodings,
            not_containing: query.not_containing,
            skip_binary: query.skip_binary,
        })
    }

    /// Reads `reader` to the end or the first match, `buf` as the read buffer.
    pub(crate) async fn scan(
        &self,
        reader: &mut (impl AsyncRead + Unpin),
        buf: &mut Vec<u8>,
        cancel: &SearchCancel,
    ) -> std::io::Result<Scan> {
        buf.resize(CHUNK, 0);
        let mut windows: Vec<Window> = Vec::new();
        let mut first = true;
        loop {
            if cancel.is_cancelled() {
                return Ok(Scan::Cancelled);
            }
            let read = if first {
                // The whole first chunk, as the viewer's detection sees a file's head.
                fill(&mut *reader, buf).await?
            } else {
                reader.read(buf).await?
            };
            let chunk = &buf[..read];
            if first {
                if self.skip_binary && self.looks_binary(chunk) {
                    return Ok(Scan::Binary);
                }
                windows = self
                    .encodings
                    .iter()
                    .map(|encoding| Window::new(encoding.decoder(chunk)))
                    .collect();
            }
            first = false;
            let last = read == 0;
            for window in &mut windows {
                if window.feed(chunk, last, self) {
                    return Ok(Scan::Found);
                }
            }
            if last {
                return Ok(Scan::Absent);
            }
        }
    }

    /// A NUL byte near the start, unless UTF-16 is wanted, announced by a BOM or, for the
    /// automatic encoding, detected.
    fn looks_binary(&self, chunk: &[u8]) -> bool {
        let utf16 = self.encodings.iter().any(|encoding| match encoding {
            TextEncoding::Utf16Le | TextEncoding::Utf16Be => true,
            TextEncoding::Auto => matches!(
                viewer::detect(chunk).encoding,
                viewer::TextEncoding::Utf16Le | viewer::TextEncoding::Utf16Be
            ),
            TextEncoding::Utf8 | TextEncoding::Windows1252 => false,
        });
        let bom = Encoding::for_bom(chunk).is_some_and(|(encoding, _)| encoding != UTF_8);
        !utf16 && !bom && chunk[..chunk.len().min(BINARY_PROBE)].contains(&0)
    }
}

/// Reads into `buf` until it is full or the file ends; the bytes read.
async fn fill(reader: &mut (impl AsyncRead + Unpin), buf: &mut [u8]) -> std::io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let read = reader.read(&mut buf[filled..]).await?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

/// The decoded text of one encoding: the kept tail of the text so far plus the newest chunk.
struct Window {
    decoder: Decoder,
    hay: String,
    /// Where the search starts; the character before it is context for `\b` only.
    from: usize,
}

impl Window {
    fn new(decoder: Decoder) -> Self {
        Self {
            decoder,
            hay: String::new(),
            from: 0,
        }
    }

    /// Decodes `bytes` onto the window and searches it; `true` on a match.
    fn feed(&mut self, bytes: &[u8], last: bool, matcher: &ContentMatcher) -> bool {
        let mut rest = bytes;
        loop {
            let room = self
                .decoder
                .max_utf8_buffer_length(rest.len())
                .unwrap_or(rest.len());
            self.hay.reserve(room);
            let (result, read, _) = self.decoder.decode_to_string(rest, &mut self.hay, last);
            rest = &rest[read..];
            if result == CoderResult::InputEmpty {
                break;
            }
        }
        let mut suspect = None;
        if let Some(found) = matcher.regex.find_at(&self.hay, self.from) {
            // A match that touches the end may change with what follows (`\b`, `$`): it is
            // held back and searched again with the next chunk.
            if last || found.end() < self.hay.len() {
                return true;
            }
            suspect = Some(found.start());
        }
        if last {
            return false;
        }
        let mut keep = self.hay.len().saturating_sub(matcher.overlap);
        if let Some(start) = suspect {
            if self.hay.len() - start > matcher.overlap + SUSPECT_LIMIT {
                return true;
            }
            keep = keep.min(start);
        }
        while !self.hay.is_char_boundary(keep) {
            keep -= 1;
        }
        let context = self.hay[..keep]
            .chars()
            .next_back()
            .map_or(0, char::len_utf8);
        self.hay.drain(..keep - context);
        self.from = context;
        false
    }
}

#[cfg(test)]
#[path = "content_tests.rs"]
mod tests;
