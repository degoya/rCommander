//! The file's bytes as UTF-8 text: the encoding the setting names, `auto` resolved by `open`,
//! decoded by `rc-plugin-text` (RC-100-22).
//!
//! Two promises keep `text` and `tokens` in step. The text of a line is the same whether the file
//! is decoded in windows (`text`) or one line at a time ([`decode_line`], for `tokens`): a line
//! break is ASCII in every encoding offered, so no character spans one, and a UTF-8 sequence cut
//! by a window waits for the next. And every non-ASCII byte of the file becomes non-ASCII text:
//! the lexer looks at ASCII alone, so it reaches the same state on the file's bytes as on their
//! text, and `open` indexes the file before it knows the encoding of `auto`.

use rc_plugin_text::Decoder;

use crate::types::Encoding;

/// The text of one line, `bytes` without its line break. `capped`: the line goes on past
/// `bytes`, so a UTF-8 sequence cut at the end is left out rather than replaced, and the text is
/// the start of the line's whole text.
pub(crate) fn decode_line(bytes: &[u8], capped: bool, encoding: Encoding, out: &mut String) {
    let mut decoder = Decoder::new(encoding.text());
    decoder.push(bytes, out);
    if !capped {
        decoder.finish(out);
    }
}

/// Whether a file is UTF-8, fed window by window: `open`'s answer for `auto`.
#[derive(Debug, Default)]
pub(crate) struct Utf8Check {
    /// The start of a sequence the last window cut.
    carry: Vec<u8>,
    invalid: bool,
}

impl Utf8Check {
    pub(crate) fn feed(&mut self, bytes: &[u8]) {
        if self.invalid {
            return;
        }
        let mut rest = bytes;
        if let Some(lead) = self.carry.first() {
            let width = sequence_width(*lead);
            let take = width.saturating_sub(self.carry.len()).min(rest.len());
            let (head, tail) = rest.split_at(take);
            self.carry.extend_from_slice(head);
            rest = tail;
            if self.carry.len() < width {
                return;
            }
            if std::str::from_utf8(&self.carry).is_err() {
                self.invalid = true;
                return;
            }
            self.carry.clear();
        }
        match std::str::from_utf8(rest) {
            Ok(_) => {}
            Err(error) if error.error_len().is_none() => {
                self.carry = rest.get(error.valid_up_to()..).unwrap_or_default().to_vec();
            }
            Err(_) => self.invalid = true,
        }
    }

    /// Valid to the end: nothing invalid, no sequence left cut.
    pub(crate) fn valid(&self) -> bool {
        !self.invalid && self.carry.is_empty()
    }
}

/// The length of the UTF-8 sequence `lead` starts; a sequence `from_utf8` left cut starts with
/// one of these.
fn sequence_width(lead: u8) -> usize {
    match lead {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 1,
    }
}

#[cfg(test)]
#[path = "encoding_tests.rs"]
mod tests;
