use super::*;
use crate::Viewer;
use crate::test_support::{TestSource, source};

fn open(text: &str) -> Viewer<TestSource<Vec<u8>>> {
    Viewer::open(source(text))
        .expect("a source in memory opens")
        .0
}

/// The line, column and message of the first error.
fn error(text: &str) -> Option<(u64, u64, &'static str)> {
    open(text)
        .parse_error()
        .map(|error| (error.line, error.column, error.message))
}

#[test]
fn documents_are_counted() {
    assert_eq!(open("").document_count(), 0);
    assert_eq!(open("# only a comment\n").document_count(), 0);
    assert_eq!(open("a: 1\n").document_count(), 1);
    assert_eq!(open("---\n").document_count(), 1);
    assert_eq!(
        open("%YAML 1.2\n---\na: 1\n---\nb: 2\n...\nc: 3\n").document_count(),
        3
    );
}

#[test]
fn well_formed_yaml_has_no_error() {
    for text in [
        "a: 1\nb:\n  c: [1, 2]\n  d:\n  - x\n  - y: z\n    w: v\ne: |\n  text\n",
        "- - a\n  - b\n- c\n",
        "key: a plain scalar\n  that goes on\nnext: 1\n",
        "? explicit\n: value\n",
        "--- |\nliteral\n--- >\nfolded\n",
    ] {
        assert_eq!(error(text), None, "{text}");
    }
}

#[test]
fn indentation_errors_name_their_line_and_column() {
    assert_eq!(
        error("a:\n    b: 1\n  c: 2\n"),
        Some((3, 3, "bad indentation"))
    );
    assert_eq!(error("a: 1\n  b: 2\n"), Some((2, 3, "bad indentation")));
    assert_eq!(
        error("a: 1\n- b\n"),
        Some((2, 1, "expected a key at this indentation"))
    );
    assert_eq!(
        error("- a\nb: 1\n"),
        Some((2, 1, "expected '- ' at this indentation"))
    );
    assert_eq!(
        error("a: 1\n\tb: 2\n"),
        Some((2, 1, "tab in the indentation"))
    );
    assert_eq!(
        error("a: b: c\n"),
        Some((1, 5, "a mapping value is not allowed here"))
    );
}

#[test]
fn what_is_left_open_at_the_end_is_an_error() {
    assert_eq!(
        error("a: \"open\n").map(|found| found.2),
        Some("unterminated quoted scalar")
    );
    assert_eq!(
        error("a: [1,\n").map(|found| found.2),
        Some("unclosed flow collection")
    );
}

#[test]
fn nesting_is_bounded() {
    let deep = "- ".repeat(MAX_DEPTH + 10) + "x\n";
    assert_eq!(error(&deep).map(|found| found.2), Some("nesting too deep"));
}

#[test]
fn large_values_and_long_collections_are_indexed_to_their_line_end() {
    let block = "  line\n".repeat(1000);
    let items = "  - i\n".repeat(PAGE as usize + 5);
    let text = format!("text: |\n{block}list:\n{items}end: 1\n");
    let viewer = open(&text);
    assert_eq!(viewer.parse_error(), None);
    let header = text.find('|').expect("a header") as u64;
    let entry = viewer
        .structure
        .entry(header)
        .expect("the block scalar is indexed");
    assert_eq!(entry.end, text.find("list:").expect("list") as u64);
    let first_item = text.find("  - i").expect("an item") as u64 + 2;
    let entry = viewer
        .structure
        .entry(first_item)
        .expect("the list is indexed");
    assert_eq!(entry.count, PAGE + 5);
    assert_eq!(entry.end, text.find("end:").expect("end") as u64);
    assert_eq!(entry.mark(PAGE).map(|mark| mark.1), Some(PAGE));
}
