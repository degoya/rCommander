use super::*;
use crate::lexer::Lexer;
use crate::reader::{Bytes, SliceBytes};

fn build(text: &str) -> Structure {
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes()));
    let mut builder = Builder::new();
    while let Some(lexeme) = lexer.next().expect("a slice cannot fail") {
        builder.feed(&lexeme);
    }
    let (line, column) = lexer.position();
    let eof = lexer.bytes_mut().offset();
    builder.finish(eof, line, column)
}

fn error(text: &str) -> Option<(u64, u64, &'static str)> {
    build(text)
        .error
        .map(|error| (error.line, error.column, error.message))
}

#[test]
fn valid_json_has_no_error_and_a_root() {
    for text in [
        "{}",
        "[]",
        " 1 ",
        "\"s\"",
        "null",
        r#"{"a": [1, {"b": null}], "c": {}}"#,
    ] {
        let structure = build(text);
        assert_eq!(structure.error, None, "{text}");
        assert_eq!(
            structure.root,
            Some(text.len() as u64 - text.trim_start().len() as u64)
        );
        assert_eq!(structure.end, text.len() as u64);
    }
}

#[test]
fn the_first_error_is_named_by_line_and_column() {
    assert_eq!(
        error("{\"a\": 1,\n  \"b\": tru\n}"),
        Some((2, 8, "invalid literal"))
    );
    assert_eq!(error("[1,]"), Some((1, 4, "expected a value")));
    assert_eq!(error("{\"a\" 1}"), Some((1, 6, "expected ':'")));
    assert_eq!(
        error("{1: 2}"),
        Some((1, 2, "expected a key in double quotes"))
    );
    assert_eq!(
        error("[1 2]"),
        Some((1, 4, "expected ',' or a closing bracket"))
    );
    assert_eq!(
        error("[1}"),
        Some((1, 3, "expected ',' or a closing bracket"))
    );
    assert_eq!(
        error("[1] 2"),
        Some((1, 5, "unexpected content after the value"))
    );
    assert_eq!(error(""), Some((1, 1, "no JSON value")));
    assert_eq!(error("  \n "), Some((2, 2, "no JSON value")));
    assert_eq!(error("[1, [2"), Some((1, 7, "unexpected end of file")));
    assert_eq!(error("[\"a\nb\"]"), Some((1, 2, "unterminated string")));
    // Columns count characters, not bytes.
    assert_eq!(
        error("{\"\u{e9}\u{e9}\": x}"),
        Some((1, 8, "invalid literal"))
    );
    let deep = "[".repeat(MAX_DEPTH + 1);
    assert_eq!(
        error(&deep),
        Some((1, MAX_DEPTH as u64 + 1, "nesting too deep"))
    );
}

#[test]
fn containers_open_at_an_error_end_where_the_valid_part_does() {
    let text = format!("[{}", "1,".repeat(1500));
    let structure = build(&text);
    let entry = structure.entry(0).expect("more than a page of children");
    assert_eq!(entry.end, structure.end);
    assert_eq!(entry.count, 1500);
}

#[test]
fn a_long_list_keeps_an_anchor_every_page() {
    let text = format!("[{}0]", "0,".repeat(2499));
    let structure = build(&text);
    let entry = structure.entry(0).expect("the list is indexed");
    assert_eq!(entry.count, 2500);
    assert_eq!(entry.end, text.len() as u64);
    assert_eq!(entry.anchor_step, PAGE);
    // Child i starts at 1 + 2 * i.
    assert_eq!(&*entry.anchors, &[2001, 4001]);
}

#[test]
fn small_values_are_not_indexed_and_large_ones_are() {
    let long = "x".repeat(FIRST_MIN_SIZE as usize);
    let text = format!(r#"{{"short": "y", "long": "{long}", "list": [1, 2]}}"#);
    let structure = build(&text);
    let long_start = text.find("\"x").expect("the long string") as u64;
    assert!(structure.entry(long_start).is_some());
    assert!(
        structure
            .entry(text.find("\"y").expect("short") as u64)
            .is_none()
    );
    assert!(
        structure
            .entry(text.find('[').expect("list") as u64)
            .is_none()
    );
    // The root holds the long string, so it is large too.
    assert_eq!(structure.entry(0).map(|entry| entry.count), Some(3));
}

#[test]
fn an_overfull_index_drops_its_smallest_entries() {
    let mut structure = Structure {
        min_size: FIRST_MIN_SIZE,
        ..Structure::default()
    };
    let mut start = 0u64;
    for index in 0..400_000u64 {
        let size = FIRST_MIN_SIZE + (index % 64) * 1024;
        structure.record(Entry {
            start,
            end: start + size,
            count: 0,
            anchor_step: PAGE,
            anchors: Box::default(),
        });
        start += size;
    }
    assert!(structure.memory_bytes() <= MAX_INDEX_BYTES);
    assert!(structure.min_size > FIRST_MIN_SIZE);
    let min_size = structure.min_size;
    assert!(
        structure
            .entries
            .iter()
            .all(|entry| entry.end - entry.start >= min_size)
    );
}

#[test]
fn anchors_of_a_huge_container_thin_to_a_longer_step() {
    let mut frame = Frame {
        start: 0,
        object: false,
        count: 0,
        anchor_step: PAGE,
        anchors: Vec::new(),
    };
    for index in 0..10_000_000u64 {
        frame.child(index * 10);
    }
    assert!(frame.anchors.len() < MAX_ANCHORS);
    assert!(frame.anchor_step > PAGE);
    for (slot, offset) in frame.anchors.iter().enumerate() {
        assert_eq!(*offset, (slot as u64 + 1) * frame.anchor_step * 10);
    }
}
