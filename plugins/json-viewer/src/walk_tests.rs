use super::*;

#[test]
fn escapes_are_resolved_for_display() {
    assert_eq!(
        decode_string(br#"a\"b\\c\/\b\f\n\r\t"#, false),
        "a\"b\\c/\u{8}\u{c}\n\r\t"
    );
    assert_eq!(
        decode_string(br#"caf\u00e9 \uD83D\uDE00"#, false),
        "caf\u{e9} \u{1F600}"
    );
    // A lone surrogate and an escape JSON does not know.
    assert_eq!(decode_string(br#"\uD83Dx\q"#, false), "\u{FFFD}x\\q");
}

#[test]
fn a_cut_string_drops_its_half_character_and_half_escape() {
    assert_eq!(
        decode_string("ab\u{e9}".as_bytes().get(..3).expect("three bytes"), true),
        "ab"
    );
    assert_eq!(decode_string(br#"ab\u00"#, true), "ab");
    assert_eq!(decode_string(br#"ab\"#, true), "ab");
}

#[test]
fn previews_quote_strings_and_cut_long_values() {
    assert_eq!(preview(br#""caf\u00e9""#, true), "\"caf\u{e9}\"");
    assert_eq!(preview(b"-1.5e3", true), "-1.5e3");
    assert_eq!(preview(b"null", true), "null");
    let long = format!("\"{}\"", "x".repeat(200));
    let shown = preview(long.as_bytes(), true);
    assert_eq!(shown.chars().count(), PREVIEW_CHARS + 2);
    assert!(shown.starts_with("\"xxx") && shown.ends_with('\u{2026}'));
    // A value read only in part ends in an ellipsis even when short.
    assert_eq!(preview(br#""abc"#, false), "\"abc\u{2026}");
}

#[test]
fn keys_lose_their_quotes() {
    assert_eq!(key_label(br#""na\"me""#, true), "na\"me");
    assert_eq!(key_label(br#""""#, true), "");
    assert_eq!(key_label(br#""abc"#, false), "abc\u{2026}");
}
