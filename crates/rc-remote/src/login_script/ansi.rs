//! Terminal escape sequences removed from the output before a pattern sees it: a coloured
//! prompt (`\e[1;31mroot\e[0m@box:~# `) matches what a person reads on the screen.
//!
//! Output arrives in blocks that can end inside a sequence or inside a UTF-8 character;
//! [`AnsiStripper`] holds such an unfinished end back until the next block completes it.

/// The escape character.
const ESC: u8 = 0x1b;

/// The bell, which also ends an OSC sequence.
const BEL: u8 = 0x07;

/// An unfinished sequence longer than this is dropped instead of held: a server that opens an
/// OSC string and never ends it cannot grow the held bytes.
const MAX_PENDING: usize = 4096;

/// Strips blocks of output one after the other.
#[derive(Debug, Default)]
pub(crate) struct AnsiStripper {
    /// The start of an escape sequence whose end has not arrived.
    pending: Vec<u8>,
    /// The first bytes of a UTF-8 character whose rest has not arrived.
    partial: Vec<u8>,
}

impl AnsiStripper {
    /// The text of `block` after what came before: escape sequences removed, an unfinished one
    /// or an unfinished character at its end held back. Invalid UTF-8 becomes U+FFFD.
    pub(crate) fn push(&mut self, block: &[u8]) -> String {
        let mut input = std::mem::take(&mut self.pending);
        input.extend_from_slice(block);
        let mut out = std::mem::take(&mut self.partial);
        let mut at = 0;
        while at < input.len() {
            if input[at] != ESC {
                out.push(input[at]);
                at += 1;
                continue;
            }
            match sequence_end(&input, at + 1) {
                Some(end) => at = end,
                None => {
                    if input.len() - at <= MAX_PENDING {
                        self.pending = input[at..].to_vec();
                    }
                    break;
                }
            }
        }
        let cut = complete_prefix(&out);
        self.partial = out[cut..].to_vec();
        String::from_utf8_lossy(&out[..cut]).into_owned()
    }
}

/// `bytes` as text without escape sequences: CSI (`ESC [ ... final`), OSC, DCS and the other
/// string sequences (ended by BEL or `ESC \`), the two-byte ones (`ESC =`) and a character-set
/// designation with its set (`ESC ( B`). An unfinished sequence or character at the end is
/// dropped. Invalid UTF-8 becomes U+FFFD.
#[must_use]
pub fn strip_ansi(bytes: &[u8]) -> String {
    AnsiStripper::default().push(bytes)
}

/// The index after the escape sequence whose introducer follows `ESC` at `at`; `None` while it
/// is unfinished.
fn sequence_end(bytes: &[u8], at: usize) -> Option<usize> {
    match bytes.get(at)? {
        b'[' => {
            // Parameters and intermediates (0x20 to 0x3f), then one final byte (0x40 to 0x7e).
            let mut next = at + 1;
            while next < bytes.len() && (0x20..=0x3f).contains(&bytes[next]) {
                next += 1;
            }
            (next < bytes.len()).then_some(next + 1)
        }
        b']' | b'P' | b'X' | b'^' | b'_' => {
            let mut next = at + 1;
            while next < bytes.len() {
                match bytes[next] {
                    BEL => return Some(next + 1),
                    ESC => return bytes.get(next + 1).map(|_| next + 2),
                    _ => next += 1,
                }
            }
            None
        }
        // A character-set designation names its set in the byte after.
        b'(' | b')' | b'*' | b'+' => (at + 1 < bytes.len()).then_some(at + 2),
        _ => Some(at + 1),
    }
}

/// The length of `bytes` without an unfinished UTF-8 character at its end.
fn complete_prefix(bytes: &[u8]) -> usize {
    // A character is at most four bytes: look at the last three for its first byte.
    for back in 1..=3_usize.min(bytes.len()) {
        let at = bytes.len() - back;
        let byte = bytes[at];
        if byte & 0xc0 == 0x80 {
            continue;
        }
        let needed = match byte {
            0xc0..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf7 => 4,
            _ => 1,
        };
        return if back < needed { at } else { bytes.len() };
    }
    bytes.len()
}

#[cfg(test)]
#[path = "ansi_tests.rs"]
mod tests;
