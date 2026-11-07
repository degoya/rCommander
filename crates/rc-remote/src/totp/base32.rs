//! RFC 4648 Base32 (`A`-`Z`, `2`-`7`) as authenticators print a secret: any case, grouped with
//! spaces or dashes, with or without `=` padding. Bits left over after the last whole byte are
//! dropped, as authenticator apps do: a secret drawn as random symbols rather than encoded from
//! bytes leaves some.

use secrecy::zeroize::Zeroizing;

/// The bytes `text` encodes; `None` for no symbol at all, a character outside the alphabet or a
/// symbol after padding.
pub(super) fn decode(text: &str) -> Option<Zeroizing<Vec<u8>>> {
    let mut out = Zeroizing::new(Vec::with_capacity(text.len() * 5 / 8));
    let mut buffer: u64 = 0;
    let mut bits = 0_u32;
    let mut padded = false;
    let mut symbols = 0_usize;
    for byte in text.bytes() {
        let value = match byte {
            b' ' | b'-' | b'\t' => continue,
            b'=' => {
                padded = true;
                continue;
            }
            _ if padded => return None,
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a',
            b'2'..=b'7' => byte - b'2' + 26,
            _ => return None,
        };
        symbols += 1;
        buffer = (buffer << 5) | u64::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xff).unwrap_or_default());
            buffer &= (1 << bits) - 1;
        }
    }
    (symbols > 0).then_some(out)
}

#[cfg(test)]
#[path = "base32_tests.rs"]
mod tests;
