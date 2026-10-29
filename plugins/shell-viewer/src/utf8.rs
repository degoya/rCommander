//! The text is UTF-8: a sequence that is not becomes U+FFFD, one for each maximal invalid part,
//! as `String::from_utf8_lossy` replaces it. [`unit`] tells the lexer what one character takes
//! in the source and in the text, so its columns count the text written; [`write_lossy`] writes
//! that text in windows.

use crate::types::{Failure, FailureKind, Sink, Source};

/// U+FFFD in UTF-8.
const REPLACEMENT: &[u8] = "\u{FFFD}".as_bytes();
/// The most one `write-at` moves.
const WRITE_CHUNK: usize = 1 << 20;

/// The bytes the character at the start of `bytes` takes in the source and in the text: an
/// ASCII byte or a valid character keeps its bytes, an invalid sequence becomes U+FFFD's three.
/// `bytes`: the next four bytes, fewer only at the end.
pub(crate) fn unit(bytes: &[u8]) -> (usize, u32) {
    let Some(&first) = bytes.first() else {
        return (0, 0);
    };
    if first < 0x80 {
        return (1, 1);
    }
    let valid = match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(error) => error.valid_up_to() > 0,
    };
    if valid {
        return match first {
            0xF0.. => (4, 4),
            0xE0.. => (3, 3),
            _ => (2, 2),
        };
    }
    let invalid = std::str::from_utf8(bytes)
        .err()
        .and_then(|error| error.error_len())
        .unwrap_or(bytes.len());
    (invalid, 3)
}

/// Writes the source from `start` into `out` as UTF-8, in windows of `chunk` bytes; the bytes
/// written.
pub(crate) fn write_lossy<S: Source>(
    src: &mut S,
    start: u64,
    chunk: u32,
    out: &mut impl Sink,
) -> Result<u64, Failure> {
    let size = src.size();
    let mut writer = Writer {
        out,
        offset: 0,
        buf: Vec::with_capacity(WRITE_CHUNK),
    };
    // An incomplete sequence at a window's end, finished by the next window.
    let mut carry: Vec<u8> = Vec::new();
    let mut at = start;
    while at < size {
        if src.should_stop() {
            return Err(Failure::of(FailureKind::Cancelled));
        }
        let length = u32::try_from((size - at).min(u64::from(chunk))).unwrap_or(chunk);
        let mut bytes = src.read_at(at, length)?;
        if bytes.is_empty() {
            break;
        }
        bytes.truncate(length as usize);
        at += bytes.len() as u64;
        if carry.is_empty() {
            carry = decode(&bytes, &mut writer)?;
        } else {
            carry.extend_from_slice(&bytes);
            carry = decode(&carry, &mut writer)?;
        }
    }
    if !carry.is_empty() {
        writer.put(REPLACEMENT)?;
    }
    writer.flush()?;
    Ok(writer.offset)
}

/// Writes the valid text of `bytes` and a U+FFFD for each invalid part; the incomplete sequence
/// at the end, if any, for the next window.
fn decode<O: Sink>(mut bytes: &[u8], writer: &mut Writer<'_, O>) -> Result<Vec<u8>, Failure> {
    loop {
        match std::str::from_utf8(bytes) {
            Ok(text) => {
                writer.put(text.as_bytes())?;
                return Ok(Vec::new());
            }
            Err(error) => {
                let (valid, rest) = bytes.split_at(error.valid_up_to());
                writer.put(valid)?;
                let Some(invalid) = error.error_len() else {
                    return Ok(rest.to_vec());
                };
                writer.put(REPLACEMENT)?;
                bytes = rest.get(invalid..).unwrap_or_default();
            }
        }
    }
}

/// The text gathered into writes of at most [`WRITE_CHUNK`].
struct Writer<'a, O: Sink> {
    out: &'a mut O,
    offset: u64,
    buf: Vec<u8>,
}

impl<O: Sink> Writer<'_, O> {
    fn put(&mut self, mut bytes: &[u8]) -> Result<(), Failure> {
        while !bytes.is_empty() {
            let room = WRITE_CHUNK - self.buf.len();
            let (now, later) = bytes.split_at(room.min(bytes.len()));
            self.buf.extend_from_slice(now);
            bytes = later;
            if self.buf.len() == WRITE_CHUNK {
                self.flush()?;
            }
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Failure> {
        if !self.buf.is_empty() {
            self.out.write_at(self.offset, &self.buf)?;
            self.offset += self.buf.len() as u64;
            self.buf.clear();
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "utf8_tests.rs"]
mod tests;
