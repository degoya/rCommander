use super::*;

#[test]
fn an_entity_is_named_decimal_or_hexadecimal_and_ends_with_a_semicolon() {
    assert_eq!(entity_len(b"&amp; x"), 5);
    assert_eq!(entity_len(b"&#38;"), 5);
    assert_eq!(entity_len(b"&#x26;"), 6);
    assert_eq!(entity_len(b"&#X2f;"), 6);
    let none: [&[u8]; 7] = [b"& x", b"&amp", b"&#;", b"&#x;", b"&#xg;", b"&;", b"&"];
    for bytes in none {
        assert_eq!(entity_len(bytes), 0, "{bytes:?}");
    }
}

#[test]
fn a_word_keeps_its_first_bytes_for_comparing() {
    let word = Word::of(b"return x", b"");
    assert_eq!(word.text(), b"return");
    assert!(word.is_in(&["if", "return"]));
    let dashed = Word::of(b"max-width:", b"-");
    assert_eq!(dashed.text(), b"max-width");
    assert!(Word::of(b"ECHO ", b"").is_in_any_case(&["echo"]));
    let long = Word::of(&[b'a'; 40], b"");
    assert!(long.text().is_empty());
    assert!(!long.is_in(&["a"]));
}

#[test]
fn a_class_name_is_capitalized_and_a_constant_is_not() {
    assert!(Word::of(b"Foo", b"").is_capitalized());
    assert!(!Word::of(b"FOO_BAR", b"").is_capitalized());
    assert!(!Word::of(b"foo", b"").is_capitalized());
    assert!(Word::of(b"T", b"").starts_upper());
}
