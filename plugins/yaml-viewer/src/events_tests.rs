use super::*;
use crate::reader::SliceBytes;

/// Kind, column, text and whether it is first on its line, of every event of `text`.
fn events(text: &str) -> Vec<(EventKind, u32, String, bool)> {
    let lexer = Lexer::new(SliceBytes::new(text.as_bytes())).with_capture(256);
    let mut events = Events::new(lexer, u64::MAX, true);
    let mut out = Vec::new();
    while let Some(event) = events.next().expect("a slice cannot fail") {
        out.push((event.kind, event.col, event.text, event.first_on_line));
    }
    out
}

fn event(kind: EventKind, col: u32, text: &str, first: bool) -> (EventKind, u32, String, bool) {
    (kind, col, text.to_owned(), first)
}

#[test]
fn keys_items_and_values_with_their_columns() {
    use EventKind::{DocStart, Item, Key, Value};
    assert_eq!(
        events("- &a key: v\n  other:\n    - x # note\n--- |\n  text\n"),
        vec![
            event(Item, 0, "", true),
            // A key starts at its anchor.
            event(Key, 2, "key", false),
            event(Value, 10, "v", false),
            event(Key, 2, "other", true),
            event(Item, 4, "", true),
            event(Value, 6, "x", false),
            event(DocStart, 0, "", true),
            event(Value, 4, "| text", false),
        ]
    );
}

#[test]
fn values_are_previewed_with_their_properties() {
    use EventKind::{Key, Value};
    let long = "y".repeat(100);
    let text = format!(
        "a: !!str 'it''s'\nb: [1, {{c: d}}]\nc: [x,\n  y]\nd: \"open\n  more\"\ne: |\n  one\n  two\nf: {long}\n"
    );
    let previews: Vec<(EventKind, String)> = events(&text)
        .into_iter()
        .map(|(kind, _, text, _)| (kind, text))
        .collect();
    let cut = format!("{}\u{2026}", "y".repeat(PREVIEW_CHARS));
    assert_eq!(
        previews,
        vec![
            (Key, "a".to_owned()),
            (Value, "!!str 'it''s'".to_owned()),
            (Key, "b".to_owned()),
            (Value, "[1, {c: d}]".to_owned()),
            (Key, "c".to_owned()),
            (Value, "[x,\u{2026}".to_owned()),
            (Key, "d".to_owned()),
            (Value, "\"open\u{2026}".to_owned()),
            (Key, "e".to_owned()),
            (Value, "| one\u{2026}".to_owned()),
            (Key, "f".to_owned()),
            (Value, cut),
        ]
    );
}

#[test]
fn quoted_keys_lose_their_quotes() {
    let found = events("\"a b\": 1\n'c': 2\n");
    let keys: Vec<&str> = found
        .iter()
        .filter(|found| found.0 == EventKind::Key)
        .map(|found| found.2.as_str())
        .collect();
    assert_eq!(keys, vec!["a b", "c"]);
}

#[test]
fn nodes_where_none_may_be_are_errors() {
    let kinds =
        |text: &str| -> Vec<EventKind> { events(text).into_iter().map(|found| found.0).collect() };
    assert_eq!(
        kinds("a: b: c\n"),
        vec![
            EventKind::Key,
            EventKind::Invalid("a mapping value is not allowed here"),
        ]
    );
    assert_eq!(
        kinds("a: - b\n"),
        vec![
            EventKind::Key,
            EventKind::Invalid("a sequence item is not allowed here"),
        ]
    );
    // A document marker closes a flow collection left open.
    assert_eq!(
        kinds("a: [1,\n---\nb: 2\n"),
        vec![
            EventKind::Key,
            EventKind::Value,
            EventKind::DocStart,
            EventKind::Key,
            EventKind::Value,
        ]
    );
}
