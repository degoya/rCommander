//! A line's bytes as text, through `rc-plugin-text`, and the check `auto` makes on the way.
//!
//! A line is decoded on its own: a newline never sits inside a character of any encoding the
//! plugin reads, so the line's text is exactly that line of the text `text` writes. Of a line
//! longer than the lexed part, a character the cut splits is left out rather than written as
//! U+FFFD, so the text stays a prefix of the line `text` writes.

use rc_plugin_text::{Decoder, Encoding};

/// Appends the text of a line's `raw` bytes to `out`; `capped` when `raw` is only its start.
pub(crate) fn decode_line(raw: &[u8], capped: bool, encoding: Encoding, out: &mut String) {
    let mut decoder = Decoder::new(encoding);
    decoder.push(raw, out);
    if !capped {
        decoder.finish(out);
    }
}

/// What the lexer reads of a line for its state alone: the bytes themselves where a code unit is
/// a byte - the lexer decides on ASCII, which every such encoding shares - else the line decoded
/// into `buf`.
pub(crate) fn line_text<'a>(
    raw: &'a [u8],
    capped: bool,
    encoding: Encoding,
    buf: &'a mut String,
) -> &'a [u8] {
    if matches!(encoding, Encoding::Utf16Le | Encoding::Utf16Be) {
        buf.clear();
        decode_line(raw, capped, encoding, buf);
        buf.as_bytes()
    } else {
        raw
    }
}

/// Whether a line's `raw` bytes are UTF-8; of a `capped` line a character cut at the end counts.
pub(crate) fn utf8_valid(raw: &[u8], capped: bool) -> bool {
    match std::str::from_utf8(raw) {
        Ok(_) => true,
        Err(error) => capped && error.error_len().is_none(),
    }
}
