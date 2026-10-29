use super::*;
use crate::test_support::TestSource;

/// Every line of `bytes` read with `newline` through windows of `chunk` bytes, and whether it
/// was capped at `cap`.
fn read_all(bytes: &[u8], newline: Newline, chunk: u32, cap: usize) -> Vec<(Vec<u8>, bool)> {
    let mut src = TestSource::new(bytes.to_vec());
    let mut reader = Reader::new(&mut src, 0, newline, chunk);
    let mut line = Vec::new();
    let mut lines = Vec::new();
    while let Some(capped) = reader.read_line(cap, &mut line).expect("reads") {
        lines.push((line.clone(), capped));
    }
    lines
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn utf16be(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_be_bytes).collect()
}

#[test]
fn lines_lose_their_break_and_a_carriage_return_before_it() {
    for chunk in [1, 2, 3, 1024] {
        let lines = read_all(b"a=1\r\n\nb=2\nlast", Newline::Byte, chunk, 64);
        let texts: Vec<&[u8]> = lines.iter().map(|(line, _)| line.as_slice()).collect();
        assert_eq!(texts, [&b"a=1"[..], b"", b"b=2", b"last"], "chunk {chunk}");
        assert!(lines.iter().all(|(_, capped)| !capped));
    }
    assert!(read_all(b"", Newline::Byte, 16, 64).is_empty());
    assert_eq!(read_all(b"x\n", Newline::Byte, 16, 64).len(), 1);
}

#[test]
fn utf16_lines_break_on_whole_units_only() {
    // U+0A41 U+0100 are the bytes 41 0A 00 01 in UTF-16 LE, U+010A is 0A 01: neither holds a
    // line break, though the bytes 0A 00 sit between the first two.
    let text = "[HKEY_X]\r\n\"a\"=\"\u{a41}\u{100}\"\r\n\u{10a}\n";
    for chunk in [1, 2, 3, 5, 1024] {
        let lines = read_all(&utf16le(text), Newline::Utf16Le, chunk, 1024);
        let texts: Vec<Vec<u8>> = lines.into_iter().map(|(line, _)| line).collect();
        assert_eq!(
            texts,
            vec![
                utf16le("[HKEY_X]"),
                utf16le("\"a\"=\"\u{a41}\u{100}\""),
                utf16le("\u{10a}")
            ],
            "chunk {chunk}"
        );
        let lines = read_all(&utf16be(text), Newline::Utf16Be, chunk, 1024);
        assert_eq!(
            lines[1].0,
            utf16be("\"a\"=\"\u{a41}\u{100}\""),
            "chunk {chunk}"
        );
    }
}

#[test]
fn an_odd_byte_at_the_end_belongs_to_the_last_line() {
    let mut bytes = utf16le("a\nb");
    bytes.push(0x41);
    let lines = read_all(&bytes, Newline::Utf16Le, 3, 64);
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[1].0, b"b\x00\x41");
}

#[test]
fn a_long_line_is_capped_and_read_past() {
    let lines = read_all(b"abcdefgh\r\nij\r\n", Newline::Byte, 3, 4);
    assert_eq!(lines[0], (b"abcd".to_vec(), true));
    assert_eq!(lines[1], (b"ij".to_vec(), false));
}

#[test]
fn a_head_already_read_is_not_read_again() {
    let bytes = b"\xEF\xBB\xBFk=v\nx=y\n".to_vec();
    let mut src = TestSource::new(bytes.clone());
    let head = bytes[..6].to_vec();
    let mut reader = Reader::new(&mut src, 3, Newline::Byte, 1024).with_head(head);
    let mut line = Vec::new();
    assert_eq!(reader.read_line(64, &mut line).expect("reads"), Some(false));
    assert_eq!(line, b"k=v");
    assert_eq!(reader.offset(), 7);
    assert_eq!(reader.read_line(64, &mut line).expect("reads"), Some(false));
    assert_eq!(line, b"x=y");
    assert_eq!(reader.read_line(64, &mut line).expect("reads"), None);
    drop(reader);
    assert_eq!(src.bytes_read, bytes.len() as u64 - 6);
}
