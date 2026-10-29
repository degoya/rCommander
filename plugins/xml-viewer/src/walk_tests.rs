use super::*;

#[test]
fn entities_decode_to_their_characters() {
    for (entity, text) in [
        ("&lt;", "<"),
        ("&gt;", ">"),
        ("&amp;", "&"),
        ("&quot;", "\""),
        ("&apos;", "'"),
        ("&#233;", "\u{e9}"),
        ("&#x1F600;", "\u{1F600}"),
        ("&nbsp;", "&nbsp;"),
        ("&#xD800;", "&#xD800;"),
        ("&#99999999999;", "&#99999999999;"),
    ] {
        assert_eq!(
            decode_entity(entity.as_bytes()),
            text.as_bytes(),
            "{entity}"
        );
    }
}

#[test]
fn text_is_folded_and_cut() {
    let mut text = Text::default();
    text.push(Kind::Text, b"a\t b", true, false);
    text.push(Kind::Entity, b"&amp;", true, false);
    text.push(Kind::CData { closed: true }, b"c ]]>", true, true);
    assert_eq!(text.finish().as_deref(), Some("a b& c"));

    let mut blank = Text::default();
    blank.push(Kind::CData { closed: true }, b"  ]]>", true, false);
    assert_eq!(blank.finish(), None);

    let mut long = Text::default();
    long.push(Kind::Text, "\u{e9}".repeat(400).as_bytes(), true, false);
    let shown = long.finish().expect("text");
    assert_eq!(shown.chars().count(), PREVIEW_CHARS + 1);
    assert!(shown.ends_with('\u{2026}'));
}

#[test]
fn shorten_cuts_at_characters() {
    assert_eq!(shorten("abc".to_owned(), 2, false), "ab\u{2026}");
    assert_eq!(shorten("ab".to_owned(), 2, false), "ab");
    assert_eq!(shorten("ab".to_owned(), 2, true), "ab\u{2026}");
}
