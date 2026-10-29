use super::*;
use crate::test_support::{TestSource, utf16le};

/// Every line of `bytes` read from `offset` through windows of `chunk` bytes.
fn read_all(bytes: Vec<u8>, offset: u64, chunk: u32, encoding: Encoding) -> Vec<Vec<u8>> {
    let mut src = TestSource::new(bytes);
    let mut reader = Reader::new(&mut src, offset, chunk, encoding);
    let mut line = Vec::new();
    let mut out = Vec::new();
    while reader
        .read_line(1 << 20, &mut line)
        .expect("the source reads")
        .is_some()
    {
        out.push(line.clone());
    }
    out
}

#[test]
fn bytes_split_at_newlines_and_lose_a_carriage_return() {
    let lines = read_all(b"a\r\nbc\n\nd".to_vec(), 0, 4, Encoding::Windows1252);
    assert_eq!(
        lines,
        vec![b"a".to_vec(), b"bc".to_vec(), Vec::new(), b"d".to_vec()]
    );
}

#[test]
fn utf16_splits_at_whole_units_only() {
    // U+0A41 U+4E00 is `41 0A 00 4E`: a `0A 00` across two units is no line break.
    let text = "x\u{0a41}\u{4e00}\r\ny\n";
    for chunk in [4, 6, 1 << 16] {
        let lines = read_all(utf16le(text, true), 2, chunk, Encoding::Utf16Le);
        assert_eq!(
            lines,
            vec![utf16le("x\u{0a41}\u{4e00}", false), utf16le("y", false)],
            "windows of {chunk}"
        );
    }
    let big_endian: Vec<u8> = "a\r\nb".encode_utf16().flat_map(u16::to_be_bytes).collect();
    assert_eq!(
        read_all(big_endian, 0, 8, Encoding::Utf16Be),
        vec![vec![0, b'a'], vec![0, b'b']]
    );
}

#[test]
fn a_line_longer_than_the_cap_is_cut_and_read_past() {
    let mut src = TestSource::new(b"abcdef\r\ngh\n".to_vec());
    let mut reader = Reader::new(&mut src, 0, 4, Encoding::Utf8);
    let mut line = Vec::new();
    assert_eq!(reader.read_line(4, &mut line).expect("reads"), Some(true));
    assert_eq!(line, b"abcd");
    assert_eq!(reader.read_line(4, &mut line).expect("reads"), Some(false));
    assert_eq!(line, b"gh");
    assert_eq!(reader.read_line(4, &mut line).expect("reads"), None);
}

#[test]
fn a_window_read_before_is_not_read_again() {
    let bytes = b"\xEF\xBB\xBFone\ntwo\n".to_vec();
    let mut src = TestSource::new(bytes.clone());
    let mut reader = Reader::new(&mut src, 3, 1 << 20, Encoding::Utf8).with_window(0, bytes);
    let mut line = Vec::new();
    assert_eq!(reader.read_line(64, &mut line).expect("reads"), Some(false));
    assert_eq!(line, b"one");
    assert_eq!(reader.offset(), 7);
    assert_eq!(reader.read_line(64, &mut line).expect("reads"), Some(false));
    assert_eq!(line, b"two");
    assert_eq!(reader.read_line(64, &mut line).expect("reads"), None);
    assert_eq!(src.reads, 0);
}
