use std::path::Path;

use super::{decode_byte, dos_extensions, encode_char, is_dos_file};
use crate::settings::DOS_EXTENSIONS_KEY;

#[test]
fn the_low_half_is_ascii() {
    for byte in 0..0x80u8 {
        assert_eq!(decode_byte(byte), char::from(byte), "{byte:#04x}");
    }
}

#[test]
fn the_letters_of_german_and_the_sharp_s_sit_where_dos_had_them() {
    let cases = [
        (0x81, '\u{fc}'),
        (0x84, '\u{e4}'),
        (0x8E, '\u{c4}'),
        (0x94, '\u{f6}'),
        (0x99, '\u{d6}'),
        (0x9A, '\u{dc}'),
        (0xE1, '\u{df}'),
        (0x80, '\u{c7}'),
        (0xFF, '\u{a0}'),
    ];
    for (byte, c) in cases {
        assert_eq!(decode_byte(byte), c, "{byte:#04x}");
    }
}

#[test]
fn bytes_0xb0_to_0xdf_are_box_drawing_and_blocks() {
    for byte in 0xB0..=0xDFu8 {
        let c = u32::from(decode_byte(byte));
        assert!((0x2500..=0x259F).contains(&c), "{byte:#04x} is U+{c:04X}");
    }
    let art = [
        (0xB0, '\u{2591}'),
        (0xB1, '\u{2592}'),
        (0xB2, '\u{2593}'),
        (0xB3, '\u{2502}'),
        (0xBA, '\u{2551}'),
        (0xBB, '\u{2557}'),
        (0xBC, '\u{255D}'),
        (0xC4, '\u{2500}'),
        (0xC5, '\u{253C}'),
        (0xC8, '\u{255A}'),
        (0xC9, '\u{2554}'),
        (0xCD, '\u{2550}'),
        (0xCE, '\u{256C}'),
        (0xDA, '\u{250C}'),
        (0xDB, '\u{2588}'),
        (0xDC, '\u{2584}'),
        (0xDD, '\u{258C}'),
        (0xDE, '\u{2590}'),
        (0xDF, '\u{2580}'),
    ];
    for (byte, c) in art {
        assert_eq!(decode_byte(byte), c, "{byte:#04x}");
    }
}

#[test]
fn every_byte_encodes_back_to_itself_and_no_character_twice() {
    for byte in 0..=0xFFu8 {
        assert_eq!(encode_char(decode_byte(byte)), Some(byte), "{byte:#04x}");
    }
    assert_eq!(encode_char('\u{20AC}'), None, "no euro sign in CP437");
    assert_eq!(
        encode_char('\u{263A}'),
        None,
        "the smiley of 0x01 is not used"
    );
}

#[test]
fn the_extension_decides_in_any_case_and_only_a_whole_word() {
    let list = "nfo diz";
    assert!(is_dos_file(Path::new("/r/release.nfo"), list));
    assert!(is_dos_file(Path::new("/r/FILE_ID.DIZ"), list));
    assert!(!is_dos_file(Path::new("/r/readme.txt"), list));
    assert!(!is_dos_file(Path::new("/r/nfo"), list), "no extension");
    assert!(!is_dos_file(Path::new("/r/x.nf"), list), "a part of a word");
    assert!(!is_dos_file(Path::new("/r/x.nfo"), ""), "the setting empty");
    assert!(is_dos_file(Path::new("/r/x.asc"), "asc"));
}

#[tokio::test]
async fn the_setting_names_the_extensions_and_its_default_is_nfo_and_diz() {
    assert_eq!(dos_extensions(None).await, "nfo diz");
    let db = rc_db::Db::open_in_memory().await.expect("db");
    assert_eq!(dos_extensions(Some(&db)).await, "nfo diz");
    db.set(DOS_EXTENSIONS_KEY, &"asc").await.expect("set");
    assert_eq!(dos_extensions(Some(&db)).await, "asc");
}
