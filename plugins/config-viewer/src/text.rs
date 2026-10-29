//! The text: the file after its byte-order mark, as UTF-8 whatever it was written in, decoded by
//! `rc-plugin-text`. `text` writes it whole, in windows; `tokens`, the open pass and the tree
//! decode it a line at a time. Both give the same text: a line break is one whole character in
//! every encoding read, so none spans one, and a character cut by a window waits for the next.

use rc_plugin_text::{Decoder, Encoding};

use crate::OPEN_CHUNK;
use crate::types::{Failure, FailureKind, Sink, Source};

/// The most one `write-at` takes.
const WRITE_CHUNK: usize = 1 << 20;

/// The text of one line, `bytes` without its line break. `capped`: the line goes on past
/// `bytes`, so a character cut at the end is left out rather than replaced.
pub(crate) fn decode_line(bytes: &[u8], capped: bool, encoding: Encoding, out: &mut String) {
    let mut decoder = Decoder::new(encoding);
    decoder.push(bytes, out);
    if !capped {
        decoder.finish(out);
    }
}

/// Writes the file from `start` into `out`, decoded from `encoding`.
pub(crate) fn write<S: Source>(
    src: &mut S,
    start: u64,
    encoding: Encoding,
    out: &mut impl Sink,
) -> Result<(), Failure> {
    let size = src.size();
    let mut decoder = Decoder::new(encoding);
    let mut text = String::new();
    let mut at = start;
    let mut written = 0u64;
    loop {
        text.clear();
        let window = u64::from(OPEN_CHUNK).min(size.saturating_sub(at));
        if window > 0 {
            if src.should_stop() {
                return Err(Failure::of(FailureKind::Cancelled));
            }
            let length = u32::try_from(window).unwrap_or(OPEN_CHUNK);
            let mut bytes = src.read_at(at, length)?;
            bytes.truncate(length as usize);
            if bytes.is_empty() {
                // A source shorter than announced ends here rather than looping.
                at = size;
            }
            at += bytes.len() as u64;
            decoder.push(&bytes, &mut text);
        }
        let last = at >= size;
        if last {
            decoder.finish(&mut text);
        }
        for piece in text.as_bytes().chunks(WRITE_CHUNK) {
            out.write_at(written, piece)?;
            written += piece.len() as u64;
        }
        if last {
            return Ok(());
        }
    }
}
