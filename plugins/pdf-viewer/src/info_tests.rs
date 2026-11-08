//! Text strings, dates, page sizes and the encryption mark.

use super::{MAX_VALUE_CHARS, names_encryption, page_size, text_string};

#[test]
fn text_strings_in_utf16_utf8_and_pdfdoc() {
    assert_eq!(text_string(b"\xFE\xFF\x00H\x00i"), "Hi");
    assert_eq!(text_string(b"\xEF\xBB\xBFK\xC3\xB6ln"), "K\u{f6}ln");
    assert_eq!(text_string(b"K\xF6ln"), "K\u{f6}ln");
    assert_eq!(text_string(b"  a\x07b  "), "a b");
    assert_eq!(text_string(&[b'x'; 2000]).chars().count(), MAX_VALUE_CHARS);
}

#[test]
fn a4_in_points_and_millimetres() {
    assert_eq!(page_size(595.0, 842.0), "595 x 842 pt (210 x 297 mm)");
}

#[test]
fn the_encryption_key_is_found_only_as_a_key() {
    assert!(names_encryption(b"trailer << /Encrypt 5 0 R >>"));
    assert!(names_encryption(b"<< /Encrypt<< /Filter /Standard >> >>"));
    assert!(!names_encryption(b"<< /EncryptMetadata false >>"));
    assert!(!names_encryption(b"plain text"));
}
