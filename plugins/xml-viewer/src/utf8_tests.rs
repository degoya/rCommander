use super::*;
use crate::test_support::VecSink;
use crate::types::TokenClass;

fn token(start: u32, len: u32) -> Token {
    Token {
        start,
        len,
        class: TokenClass::String,
    }
}

#[test]
fn a_valid_line_keeps_its_spans() {
    let mut tokens = vec![token(0, 1), token(1, 6)];
    to_text("<\"\u{e4}\u{20ac}\">".as_bytes(), &mut tokens);
    assert_eq!(tokens, vec![token(0, 1), token(1, 6)]);
}

#[test]
fn spans_move_past_invalid_parts_and_grow_over_them() {
    // An invalid byte before a token, a cut character inside one.
    let line = b"<\xFF \"a\xE2\x82b\">";
    let text = String::from_utf8_lossy(line);
    let mut tokens = vec![token(0, 1), token(1, 1), token(3, 6), token(9, 1)];
    to_text(line, &mut tokens);
    assert_eq!(
        tokens,
        vec![token(0, 1), token(1, 3), token(5, 7), token(12, 1)]
    );
    for token in &tokens {
        let end = (token.start + token.len) as usize;
        assert!(text.is_char_boundary(token.start as usize) && text.is_char_boundary(end));
    }
    assert_eq!(&text[5..12], "\"a\u{fffd}b\"");
    assert_eq!(&text[12..], ">");
}

#[test]
fn an_offset_inside_an_invalid_part_moves_to_its_end() {
    // F0 9F 98 is one invalid part: a span inside it is empty in the text and dropped.
    let line = b"a\xF0\x9F\x98b";
    let mut tokens = vec![token(2, 1), token(4, 1)];
    to_text(line, &mut tokens);
    assert_eq!(tokens, vec![token(4, 1)]);
}

#[test]
fn the_writer_writes_utf8_however_the_bytes_arrive() {
    let bytes = b"a\xFF\xE2\x82\xACb\n\xE2\x82";
    let mut sink = VecSink::default();
    let mut writer = TextWriter::new(&mut sink);
    for byte in bytes {
        writer.push(std::slice::from_ref(byte)).expect("push");
    }
    writer.finish().expect("finish");
    assert_eq!(sink.0, String::from_utf8_lossy(bytes).as_bytes());
}
