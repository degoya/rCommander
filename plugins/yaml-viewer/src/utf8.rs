//! The text is UTF-8 (RC-100-22): the file's bytes are written with each maximal invalid part as
//! U+FFFD, as `String::from_utf8_lossy` writes it (`rc-plugin-text`'s decoder), and a line's
//! tokens, found in its bytes, are moved onto that text, so their offsets fall on its character
//! boundaries and colour the same characters.

use rc_plugin_text::{Decoder, Encoding};

use crate::types::{Failure, Sink, Token};

/// The most one `write-at` takes.
const WRITE_CHUNK: usize = 1 << 20;
/// U+FFFD's length in UTF-8: what an invalid part takes in the text, however long it is.
const REPLACEMENT_LEN: usize = 3;

/// Writes bytes into a sink as UTF-8 text, in writes of at most [`WRITE_CHUNK`].
pub(crate) struct TextWriter<'o, O: Sink> {
    out: &'o mut O,
    decoder: Decoder,
    buf: String,
    written: u64,
}

impl<'o, O: Sink> TextWriter<'o, O> {
    pub(crate) fn new(out: &'o mut O) -> Self {
        Self {
            out,
            decoder: Decoder::new(Encoding::Utf8),
            buf: String::new(),
            written: 0,
        }
    }

    pub(crate) fn push(&mut self, bytes: &[u8]) -> Result<(), Failure> {
        self.decoder.push(bytes, &mut self.buf);
        if self.buf.len() >= WRITE_CHUNK {
            self.flush()?;
        }
        Ok(())
    }

    /// Writes the text gathered so far; a character still cut waits for the next bytes.
    pub(crate) fn flush(&mut self) -> Result<(), Failure> {
        for chunk in self.buf.as_bytes().chunks(WRITE_CHUNK) {
            self.out.write_at(self.written, chunk)?;
            self.written += chunk.len() as u64;
        }
        self.buf.clear();
        Ok(())
    }

    /// The end of the text: a character still cut is U+FFFD.
    pub(crate) fn finish(mut self) -> Result<(), Failure> {
        self.decoder.finish(&mut self.buf);
        self.flush()
    }
}

/// Moves `tokens`, spans of `line`'s bytes in order and not overlapping, onto the line's text; a
/// span left empty there is dropped. A line that is valid UTF-8 keeps its spans as they are.
pub(crate) fn to_text(line: &[u8], tokens: &mut Vec<Token>) {
    let parts = invalid_parts(line);
    if parts.is_empty() {
        return;
    }
    let mut map = OffsetMap {
        parts: &parts,
        next: 0,
        floor: 0,
        grown: 0,
        shrunk: 0,
    };
    tokens.retain_mut(|token| {
        let start = token.start as usize;
        let end = start + token.len as usize;
        let (start, end) = (map.text(start), map.text(end));
        let (Ok(start), Ok(len)) = (u32::try_from(start), u32::try_from(end - start)) else {
            return false;
        };
        token.start = start;
        token.len = len;
        len > 0
    });
}

/// The maximal invalid parts of `line`, `start..end` in its bytes; a character cut by the line's
/// end is one.
fn invalid_parts(line: &[u8]) -> Vec<(usize, usize)> {
    let mut parts = Vec::new();
    let mut at = 0;
    while let Some(rest) = line.get(at..) {
        let Err(error) = std::str::from_utf8(rest) else {
            break;
        };
        let start = at + error.valid_up_to();
        let length = error
            .error_len()
            .unwrap_or(rest.len() - error.valid_up_to());
        parts.push((start, start + length));
        at = start + length;
    }
    parts
}

/// Byte offsets of a line to offsets of its text, asked in order.
struct OffsetMap<'p> {
    parts: &'p [(usize, usize)],
    /// The first part not yet passed.
    next: usize,
    /// The end of the last part passed: an offset inside it moves there.
    floor: usize,
    grown: usize,
    shrunk: usize,
}

impl OffsetMap<'_> {
    fn text(&mut self, raw: usize) -> usize {
        let mut raw = raw.max(self.floor);
        while let Some(&(start, end)) = self.parts.get(self.next) {
            if start >= raw {
                break;
            }
            raw = raw.max(end);
            self.floor = end;
            self.grown += REPLACEMENT_LEN;
            self.shrunk += end - start;
            self.next += 1;
        }
        raw + self.grown - self.shrunk
    }
}

#[cfg(test)]
#[path = "utf8_tests.rs"]
mod tests;
