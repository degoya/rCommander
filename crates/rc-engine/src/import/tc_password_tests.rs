use std::fmt::Write as _;

use rc_vault::ExposeSecret as _;

use super::{DelphiRandom, TcPassword, classify, reveal};

/// Total Commander's obfuscation, the inverse of `decode_bytes`, with four check bytes appended:
/// the form the passwords of the fixtures (`tests/fixtures/total_commander/`) are in.
pub(crate) fn obfuscate(password: &[u8]) -> String {
    let mut bytes = password.to_vec();
    let len = bytes.len();
    let mut random = DelphiRandom(54_321);
    for byte in &mut bytes {
        *byte = byte.wrapping_add(u8::try_from(random.next(256)).expect("a byte"));
    }
    let mut random = DelphiRandom(42_340);
    for byte in &mut bytes {
        *byte ^= u8::try_from(random.next(256)).expect("a byte");
    }
    let mut random = DelphiRandom(12_345);
    let swaps: Vec<(usize, usize)> = (0..256)
        .map(|_| (random.below(len), random.below(len)))
        .collect();
    for (a, b) in swaps.into_iter().rev() {
        bytes.swap(a, b);
    }
    let mut random = DelphiRandom(849_521);
    for byte in &mut bytes {
        *byte = byte.rotate_right(random.next(8));
    }
    bytes.extend_from_slice(&[0x12, 0x34, 0x56, 0x78]);
    bytes.iter().fold(String::new(), |mut hex, byte| {
        let _ = write!(hex, "{byte:02X}");
        hex
    })
}

#[test]
fn the_kind_of_a_value_is_told_without_decoding() {
    assert_eq!(classify(None), TcPassword::None);
    assert_eq!(classify(Some("  ")), TcPassword::None);
    assert_eq!(classify(Some("!0A1B2C3D4E5F")), TcPassword::MasterPassword);
    assert_eq!(classify(Some("0A1B2C3D4E")), TcPassword::Present);
    assert_eq!(classify(Some("0A1B2C3D4")), TcPassword::Unreadable);
    assert_eq!(classify(Some("0A1B2C3D")), TcPassword::Unreadable);
    assert_eq!(classify(Some("plain text")), TcPassword::Unreadable);
}

#[test]
fn an_obfuscated_password_decodes_back() {
    for password in ["a", "hunter2", "tc-canary-7f3a9", "with space and = sign"] {
        let hex = obfuscate(password.as_bytes());
        assert_ne!(hex.to_lowercase(), password.to_lowercase());
        let revealed = reveal(&hex).expect("decodes");
        assert_eq!(revealed.expose_secret(), password);
        // Lower-case hex digits read the same.
        let lower = reveal(&hex.to_lowercase()).expect("decodes");
        assert_eq!(lower.expose_secret(), password);
    }
}

#[test]
fn ansi_bytes_are_windows_1252() {
    let hex = obfuscate(b"Gr\xFC\xDFe");
    assert_eq!(
        reveal(&hex).expect("decodes").expose_secret(),
        "Gr\u{fc}\u{df}e"
    );
}

#[test]
fn a_master_password_value_or_control_characters_reveal_nothing() {
    assert!(reveal("!0A1B2C3D4E5F").is_none());
    assert!(reveal("not hex at all").is_none());
    assert!(reveal(&obfuscate(b"line\nbreak")).is_none());
}
