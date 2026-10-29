use super::*;
use crate::test_support::source;

/// Every line of `text` as (bytes kept, how it ended), read through windows of `chunk` bytes.
fn read_all(text: &str, chunk: u32, keep: usize) -> Vec<(String, End)> {
    let mut src = source(text);
    let mut reader = LineReader::new(&mut src, 0, chunk);
    let mut head = Vec::new();
    let mut lines = Vec::new();
    while let Some(end) = reader.next_line(&mut head, keep).expect("read") {
        if end == End::Cut {
            reader.skip_line().expect("skipped");
        }
        lines.push((String::from_utf8(head.clone()).expect("UTF-8"), end));
    }
    lines
}

#[test]
fn lines_end_at_a_newline_or_at_the_end() {
    let lines = vec![
        ("one".to_owned(), End::Newline),
        (String::new(), End::Newline),
        ("three".to_owned(), End::Eof),
    ];
    // Windows of a few bytes split lines; the line comes out whole.
    for chunk in [1, 2, 3, 64] {
        assert_eq!(read_all("one\n\nthree", chunk, 64), lines, "chunk {chunk}");
    }
    assert_eq!(
        read_all("one\n", 2, 64),
        vec![("one".to_owned(), End::Newline)]
    );
    assert!(read_all("", 2, 64).is_empty());
}

#[test]
fn a_long_line_is_cut_and_the_next_one_read_whole() {
    for chunk in [1, 4, 64] {
        assert_eq!(
            read_all("abcdefgh\nxy\nabcd", chunk, 4),
            vec![
                ("abcd".to_owned(), End::Cut),
                ("xy".to_owned(), End::Newline),
                ("abcd".to_owned(), End::Eof),
            ],
            "chunk {chunk}"
        );
    }
}

#[test]
fn a_seek_goes_on_at_a_line_start() {
    let mut src = source("first line\nsecond\nthird\n");
    let mut reader = LineReader::new(&mut src, 0, 4);
    let mut head = Vec::new();
    assert_eq!(
        reader.next_line(&mut head, 3).expect("read"),
        Some(End::Cut)
    );
    reader.seek(18);
    assert_eq!(
        reader.next_line(&mut head, 64).expect("read"),
        Some(End::Newline)
    );
    assert_eq!(head, b"third");
    assert_eq!(reader.offset(), 24);
    assert_eq!(reader.next_line(&mut head, 64).expect("read"), None);
}
