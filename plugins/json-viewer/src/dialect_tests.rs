use super::*;
use crate::test_support::{TestSource, VecSink, named};
use crate::types::TokenClass::{self, Comment, Invalid, Key, Number, Punctuation};
use crate::types::{Format, TreeNode};
use crate::{Viewer, probe};

type Doc = Viewer<TestSource<Vec<u8>>>;

fn open(text: &str, name: &str, format: Format) -> Doc {
    Viewer::open(named(text, name), format)
        .expect("the document opens")
        .0
}

/// The text and class of every token of lines `first..first + count`.
fn spans(viewer: &mut Doc, text: &str, first: u32, count: u32) -> Vec<Vec<(String, TokenClass)>> {
    let lines: Vec<&str> = text.lines().collect();
    let tokens = viewer.tokens(first, count).expect("tokens");
    tokens
        .iter()
        .zip(lines.iter().skip(first as usize))
        .map(|(line_tokens, line)| {
            line_tokens
                .iter()
                .map(|token| {
                    let start = token.start as usize;
                    let end = start + token.len as usize;
                    (line[start..end].to_owned(), token.class)
                })
                .collect()
        })
        .collect()
}

fn span(text: &str, class: TokenClass) -> (String, TokenClass) {
    (text.to_owned(), class)
}

/// Label and value of every node.
fn labels(nodes: &[TreeNode]) -> Vec<(&str, Option<&str>)> {
    nodes
        .iter()
        .map(|node| (node.label.as_str(), node.value.as_deref()))
        .collect()
}

#[test]
fn the_extension_picks_the_dialect() {
    for (name, dialect) in [
        ("a.json", Dialect::Json),
        ("geo.geojson", Dialect::Json),
        ("settings.jsonc", Dialect::Jsonc),
        ("CONFIG.JSONC", Dialect::Jsonc),
        ("app.json5", Dialect::Json5),
        ("log.jsonl", Dialect::Lines),
        ("events.NDJSON", Dialect::Lines),
        ("jsonc", Dialect::Json),
        ("", Dialect::Json),
    ] {
        assert_eq!(Dialect::from_name(name), dialect, "{name}");
    }
}

const JSONC: &str = "// settings\n{\n  /* a block\n     over lines */\n  \"a\": 1, // trailing\n  \"b\": [1, 2,],\n}\n";

#[test]
fn jsonc_takes_comments_and_trailing_commas() {
    let mut viewer = open(JSONC, "settings.jsonc", Format::AsIs);
    assert_eq!(viewer.dialect(), Dialect::Jsonc);
    assert_eq!(viewer.parse_error(), None);
    assert_eq!(viewer.line_count(), 7);
    let lines = spans(&mut viewer, JSONC, 0, 7);
    assert_eq!(lines[0], vec![span("// settings", Comment)]);
    assert_eq!(lines[2], vec![span("/* a block", Comment)]);
    // The line inside the comment is one comment token, its indentation included.
    assert_eq!(lines[3], vec![span("     over lines */", Comment)]);
    assert_eq!(
        lines[4],
        vec![
            span("\"a\"", Key),
            span(":", Punctuation),
            span("1", Number),
            span(",", Punctuation),
            span("// trailing", Comment),
        ]
    );
    let top = viewer.children(None).expect("top");
    assert_eq!(labels(&top), vec![("a", Some("1")), ("b", Some("[2]"))]);
    // The same text as plain JSON stops at its first comment.
    let json = open(JSONC, "settings.json", Format::AsIs);
    let error = json.parse_error().expect("JSON has no comments");
    assert_eq!((error.line, error.column), (1, 1));
}

#[test]
fn a_line_far_inside_a_comment_is_coloured_as_one() {
    let text = format!("{{\n/*\n{}*/\n\"a\": 1}}\n", "x\n".repeat(100));
    let mut viewer = open(&text, "long.jsonc", Format::AsIs);
    assert_eq!(viewer.parse_error(), None);
    assert_eq!(viewer.line_count(), 104);
    // Line 10 from the first checkpoint, line 70 from the one inside the comment at line 64.
    for line in [10, 70, 101] {
        assert_eq!(
            spans(&mut viewer, &text, line, 1),
            vec![vec![span("x", Comment)]],
            "line {line}"
        );
    }
    assert_eq!(
        spans(&mut viewer, &text, 102, 2),
        vec![
            vec![span("*/", Comment)],
            vec![
                span("\"a\"", Key),
                span(":", Punctuation),
                span("1", Number),
                span("}", Punctuation),
            ],
        ]
    );
}

#[test]
fn an_unterminated_comment_is_the_error() {
    let viewer = open("{\"a\": 1 /* open\n", "a.jsonc", Format::AsIs);
    let error = viewer.parse_error().expect("the comment never ends");
    assert_eq!(
        (error.line, error.column, error.message),
        (1, 9, "unterminated comment")
    );
}

#[test]
fn pretty_jsonc_lays_out_the_values_without_the_comments() {
    let mut viewer = open(JSONC, "settings.jsonc", Format::Pretty);
    let mut sink = VecSink::default();
    viewer.text(&mut sink).expect("text");
    let text = String::from_utf8(sink.0).expect("UTF-8");
    assert_eq!(text, "{\n  \"a\": 1,\n  \"b\": [\n    1,\n    2,\n  ],\n}");
    assert_eq!(viewer.line_count(), text.lines().count() as u64);
}

const JSON5: &str = "// JSON5\n{\n  unquoted: 'single \"quoted\"',\n  $id: 0x1F,\n  half: .5,\n  whole: 5.,\n  plus: +1,\n  big: Infinity,\n  low: -Infinity,\n  nan: NaN,\n  'quoted key': \"it's\",\n  null: true,\n  list: [1, 2,],\n}\n";

#[test]
fn json5_takes_bare_keys_single_quotes_and_its_numbers() {
    let mut viewer = open(JSON5, "app.json5", Format::AsIs);
    assert_eq!(viewer.parse_error(), None);
    let top = viewer.children(None).expect("top");
    assert_eq!(
        labels(&top),
        vec![
            ("unquoted", Some("'single \"quoted\"'")),
            ("$id", Some("0x1F")),
            ("half", Some(".5")),
            ("whole", Some("5.")),
            ("plus", Some("+1")),
            ("big", Some("Infinity")),
            ("low", Some("-Infinity")),
            ("nan", Some("NaN")),
            ("quoted key", Some("\"it's\"")),
            ("null", Some("true")),
            ("list", Some("[2]")),
        ]
    );
    let lines = spans(&mut viewer, JSON5, 2, 8);
    assert_eq!(
        lines[0],
        vec![
            span("unquoted", Key),
            span(":", Punctuation),
            span("'single \"quoted\"'", TokenClass::String),
            span(",", Punctuation),
        ]
    );
    assert_eq!(lines[1][2], span("0x1F", Number));
    assert_eq!(lines[5][2], span("Infinity", Number));
    assert_eq!(lines[6][2], span("-Infinity", Number));
}

#[test]
fn a_json5_name_is_a_key_and_never_a_value() {
    let text = "{a: b}";
    let mut viewer = open(text, "a.json5", Format::AsIs);
    let error = viewer.parse_error().expect("a name is no value");
    assert_eq!((error.column, error.message), (5, "expected a value"));
    assert_eq!(
        spans(&mut viewer, text, 0, 1),
        vec![vec![
            span("{", Punctuation),
            span("a", Key),
            span(":", Punctuation),
            span("b", Invalid),
            span("}", Punctuation),
        ]]
    );
    // In JSON neither is valid.
    let json = open(text, "a.json", Format::AsIs);
    assert_eq!(json.parse_error().map(|error| error.column), Some(2));
}

const LINES: &str = "{\"a\": 1}\n{\"b\" 2}\n\n[1, 2]\n[1,\n";

#[test]
fn json_lines_checks_and_colours_each_line_for_itself() {
    let mut viewer = open(LINES, "log.jsonl", Format::AsIs);
    assert_eq!(viewer.dialect(), Dialect::Lines);
    let first = viewer.parse_error().expect("a broken line");
    assert_eq!((first.line, first.column), (2, 6));
    let lines = spans(&mut viewer, LINES, 0, 5);
    assert_eq!(
        lines[0],
        vec![
            span("{", Punctuation),
            span("\"a\"", Key),
            span(":", Punctuation),
            span("1", Number),
            span("}", Punctuation),
        ]
    );
    // Invalid from where the line stops being a value, and only that line.
    assert_eq!(
        lines[1],
        vec![
            span("{", Punctuation),
            span("\"b\"", TokenClass::String),
            span("2", Invalid),
            span("}", Invalid),
        ]
    );
    assert_eq!(lines[3].iter().filter(|t| t.1 == Invalid).count(), 0);
    assert_eq!(
        lines[4],
        vec![
            span("[", Punctuation),
            span("1", Number),
            span(",", Invalid)
        ]
    );
}

#[test]
fn the_tree_of_json_lines_lists_the_lines() {
    let mut viewer = open(LINES, "log.ndjson", Format::AsIs);
    let top = viewer.children(None).expect("top");
    assert_eq!(
        labels(&top),
        vec![
            ("[0]", Some("{1}")),
            ("[1]", Some("invalid: line 2, column 6: expected ':'")),
            ("[2]", Some("[2]")),
            (
                "[3]",
                Some("invalid: line 5, column 3: unexpected end of file")
            ),
        ]
    );
    let ids: Vec<u64> = top.iter().map(|node| node.id).collect();
    assert_eq!(ids, vec![0, 9, 18, 25]);
    assert!(!top[1].has_children);
    let a = viewer.children(Some(0)).expect("the first line");
    assert_eq!(labels(&a), vec![("a", Some("1"))]);
    assert!(viewer.children(Some(9)).expect("broken").is_empty());
    let list = viewer.children(Some(18)).expect("the list");
    assert_eq!(labels(&list), vec![("[0]", Some("1")), ("[1]", Some("2"))]);
}

#[test]
fn a_long_json_lines_file_unfolds_in_ranges() {
    let record = "{\"i\": 1}\n";
    let text = record.repeat(2500);
    let mut viewer = open(&text, "big.jsonl", Format::AsIs);
    let top = viewer.children(None).expect("top");
    assert_eq!(
        labels(&top),
        vec![
            ("[0..999]", None),
            ("[1000..1999]", None),
            ("[2000..2499]", None)
        ]
    );
    let last = viewer.children(Some(top[2].id)).expect("the last range");
    assert_eq!(last.len(), 500);
    assert_eq!(last[0].label, "[2000]");
    assert_eq!(last[0].id, 2000 * record.len() as u64);
    assert_eq!(last[499].label, "[2499]");
}

#[test]
fn probe_takes_a_comment_or_a_json5_value_in_their_dialects() {
    assert!(probe(&mut named("// c\n{}", "a.jsonc")));
    assert!(probe(&mut named("/* c */ {}", "a.json5")));
    assert!(probe(&mut named("'s'", "a.json5")));
    assert!(probe(&mut named("{}\n{}", "a.jsonl")));
    assert!(!probe(&mut named("// c\n{}", "a.json")));
    assert!(!probe(&mut named("'s'", "a.jsonc")));
    assert!(!probe(&mut named("# title", "a.json5")));
}
