use super::*;
use crate::dialect::Dialect;
use crate::lexer::Lexer;
use crate::reader::{Bytes, SliceBytes};

fn build(text: &str) -> Structure {
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes())).dialect(Dialect::Lines);
    let mut builder = Builder::for_dialect(Dialect::Lines, 0);
    while let Some(lexeme) = lexer.next().expect("a slice cannot fail") {
        builder.feed(&lexeme);
    }
    let (line, column) = lexer.position();
    let eof = lexer.bytes_mut().offset();
    builder.finish(eof, line, column)
}

fn count(structure: &Structure) -> u64 {
    structure
        .records
        .as_ref()
        .map_or(0, |records| records.count)
}

/// Every broken line: its start, and the line, column and message of its error.
fn broken(structure: &Structure) -> Vec<(u64, u64, u64, &'static str)> {
    structure
        .broken
        .iter()
        .map(|broken| {
            (
                broken.offset,
                broken.error.line,
                broken.error.column,
                broken.error.message,
            )
        })
        .collect()
}

#[test]
fn every_line_is_a_value_of_the_list() {
    let text = "{\"a\": 1}\n[1, 2]\n\"s\"\n3\r\nnull\n";
    let structure = build(text);
    assert_eq!(count(&structure), 5);
    assert_eq!(structure.error, None);
    assert!(structure.broken.is_empty());
    assert_eq!(structure.end, text.len() as u64);
}

#[test]
fn a_broken_line_is_broken_alone_and_the_next_reads_as_usual() {
    let text = "{\"a\": 1}\n{\"b\" 2}\n{\"c\": 3}\n";
    let structure = build(text);
    assert_eq!(count(&structure), 3);
    assert_eq!(broken(&structure), vec![(9, 2, 6, "expected ':'")]);
    assert_eq!(structure.broken_at(9).map(|b| b.offset), Some(9));
    assert_eq!(structure.broken_at(0), None);
    // A broken line ends no list: the error of the document is not set.
    assert_eq!(structure.error, None);
    assert_eq!(structure.end, text.len() as u64);
}

#[test]
fn a_value_over_two_lines_and_two_values_on_one_are_broken() {
    let structure = build("[1,\n2]\n1 2\n");
    assert_eq!(count(&structure), 3);
    assert_eq!(
        broken(&structure),
        vec![
            (0, 1, 3, "unexpected end of line"),
            (4, 2, 2, "unexpected content after the value"),
            (7, 3, 3, "unexpected content after the value"),
        ]
    );
}

#[test]
fn blank_lines_are_no_values_and_an_empty_file_is_an_empty_list() {
    let structure = build("\n\n  1\n\n\t\n2");
    assert_eq!(count(&structure), 2);
    assert!(structure.broken.is_empty());
    let empty = build("");
    assert_eq!(count(&empty), 0);
    assert_eq!(empty.error, None);
}

#[test]
fn a_last_line_cut_short_is_broken_at_the_end_of_the_file() {
    let structure = build("1\n{\"a\": [1");
    assert_eq!(count(&structure), 2);
    assert_eq!(
        broken(&structure),
        vec![(2, 2, 8, "unexpected end of file")]
    );
}

#[test]
fn a_long_list_keeps_an_anchor_every_page() {
    let text = "{}\n".repeat(2500);
    let structure = build(&text);
    let records = structure.records.as_ref().expect("the list");
    assert_eq!(records.count, 2500);
    assert_eq!(records.anchor_step, PAGE);
    assert_eq!(&*records.anchors, &[3000, 6000]);
}

#[test]
fn too_many_broken_lines_end_the_list() {
    let text = "x\n".repeat(MAX_BROKEN + 3);
    let structure = build(&text);
    assert_eq!(count(&structure), MAX_BROKEN as u64);
    assert_eq!(structure.broken.len(), MAX_BROKEN);
    let error = structure.error.as_ref().expect("the list ends");
    assert_eq!(error.message, "too many invalid lines");
    assert_eq!(error.offset, 2 * MAX_BROKEN as u64);
    assert_eq!(error.line, MAX_BROKEN as u64 + 1);
    assert_eq!(structure.end, error.offset);
}
