//! The passwords of `wcx_ftp.ini` (RC-050-09).
//!
//! Total Commander keeps an FTP password either obfuscated - hex digits that its fixed key
//! stream turns back into the password, so the value is a secret in clear - or, with its own
//! master password set, encrypted behind a leading `!`, which nothing here can or tries to read.
//! [`classify`] tells which without decoding anything: the preview and MCP only learn whether a
//! password is there. [`reveal`] decodes one, for the one command that takes it straight into the
//! vault after the person asked (`import.total_commander_passwords`, `GuiOnly`); every buffer it
//! uses is zeroized.
//!
//! The decoding follows Total Commander's published scheme: the bytes are rotated, shuffled,
//! XORed and shifted with Delphi's `Random` from four fixed seeds; the last four bytes are a
//! check value that is not needed to decode.

use rc_vault::SecretString;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::viewer::{TextEncoding, encoding::decode};

/// What `wcx_ftp.ini` holds as a connection's password - never the value itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TcPassword {
    /// None stored.
    #[default]
    None,
    /// Obfuscated: can be taken into the vault after the person asks.
    Present,
    /// Encrypted with Total Commander's master password: not read; re-enter it.
    MasterPassword,
    /// Something that is neither: not read.
    Unreadable,
}

/// The shortest obfuscated value: one byte of password and the four of the check value.
const MIN_HEX: usize = 10;

/// What the value of `password=` is, without decoding it.
#[must_use]
pub fn classify(value: Option<&str>) -> TcPassword {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return TcPassword::None;
    };
    if value.starts_with('!') {
        TcPassword::MasterPassword
    } else if value.len() >= MIN_HEX
        && value.len() % 2 == 0
        && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        TcPassword::Present
    } else {
        TcPassword::Unreadable
    }
}

/// Delphi's `Random(range)` on its 32-bit seed.
struct DelphiRandom(u32);

impl DelphiRandom {
    fn next(&mut self, range: u32) -> u32 {
        self.0 = self.0.wrapping_mul(0x0808_8405).wrapping_add(1);
        u32::try_from((u64::from(self.0) * u64::from(range)) >> 32).unwrap_or(0)
    }

    /// `next(range)` as an index or a byte; `range` is at most 256 or a buffer's length.
    fn below(&mut self, range: usize) -> usize {
        usize::try_from(self.next(u32::try_from(range).unwrap_or(u32::MAX))).unwrap_or(0)
    }
}

/// The bytes of an obfuscated value without the check value, decoded in place.
fn decode_bytes(bytes: &mut [u8]) {
    let len = bytes.len();
    let mut random = DelphiRandom(849_521);
    for byte in bytes.iter_mut() {
        *byte = byte.rotate_left(random.next(8));
    }
    let mut random = DelphiRandom(12_345);
    for _ in 0..256 {
        let (a, b) = (random.below(len), random.below(len));
        bytes.swap(a, b);
    }
    let mut random = DelphiRandom(42_340);
    for byte in bytes.iter_mut() {
        *byte ^= u8::try_from(random.next(256)).unwrap_or(0);
    }
    let mut random = DelphiRandom(54_321);
    for byte in bytes.iter_mut() {
        *byte = byte.wrapping_sub(u8::try_from(random.next(256)).unwrap_or(0));
    }
}

/// The password an obfuscated value stands for: UTF-8 when it is, else Windows-1252; `None`
/// when the value is not an obfuscated one or the result holds control characters.
#[must_use]
pub(crate) fn reveal(value: &str) -> Option<SecretString> {
    let value = value.trim();
    if classify(Some(value)) != TcPassword::Present {
        return None;
    }
    let mut bytes = Zeroizing::new(Vec::with_capacity(value.len() / 2));
    for pair in value.as_bytes().chunks(2) {
        let text = std::str::from_utf8(pair).ok()?;
        bytes.push(u8::from_str_radix(text, 16).ok()?);
    }
    let len = bytes.len() - 4;
    decode_bytes(&mut bytes[..len]);
    // A password's bytes are not a file: no byte-order mark, never UTF-16, so the viewer's
    // detection is not used here, only its decoding.
    let encoding = if std::str::from_utf8(&bytes[..len]).is_ok() {
        TextEncoding::Utf8
    } else {
        TextEncoding::Windows1252
    };
    let text = Zeroizing::new(decode(&bytes[..len], encoding));
    if text.chars().any(char::is_control) {
        return None;
    }
    Some(SecretString::from(text.as_str()))
}

#[cfg(test)]
#[path = "tc_password_tests.rs"]
pub(crate) mod tests;
