use super::*;
use crate::reader::SliceBytes;

fn lex(text: &str) -> (Vec<Lexeme>, Vec<(u64, u64)>) {
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes()));
    let mut lexemes = Vec::new();
    let mut starts = Vec::new();
    while let Some(lexeme) = lexer
        .next_with(&mut |line, offset| starts.push((line, offset)))
        .expect("a slice cannot fail")
    {
        lexemes.push(lexeme);
    }
    (lexemes, starts)
}

#[test]
fn lexemes_carry_their_line_and_column_in_characters() {
    let (lexemes, starts) = lex("{\r\n  \"\u{e9}\u{e9}\": tru,\n\t[]\n}");
    let kinds: Vec<Kind> = lexemes.iter().map(|lexeme| lexeme.kind).collect();
    assert_eq!(
        kinds,
        vec![
            Kind::BeginObject,
            Kind::String,
            Kind::Colon,
            Kind::Invalid(Problem::BadLiteral),
            Kind::Comma,
            Kind::BeginArray,
            Kind::EndArray,
            Kind::EndObject,
        ]
    );
    let positions: Vec<(u64, u64)> = lexemes
        .iter()
        .map(|lexeme| (lexeme.line, lexeme.column))
        .collect();
    assert_eq!(
        positions,
        vec![
            (1, 1),
            (2, 3),
            (2, 7),
            (2, 9),
            (2, 12),
            (3, 2),
            (3, 3),
            (4, 1)
        ]
    );
    assert_eq!(starts, vec![(1, 3), (2, 18), (3, 22)]);
}

#[test]
fn a_string_never_runs_past_its_line() {
    let (lexemes, _) = lex("[\"open\r\n, 1]");
    assert_eq!(lexemes[1].kind, Kind::Invalid(Problem::UnterminatedString));
    assert_eq!((lexemes[1].start, lexemes[1].end), (1, 6));
    assert_eq!(lexemes[2].kind, Kind::Comma);
    assert_eq!(lexemes[2].line, 2);
}

#[test]
fn the_capture_keeps_the_first_bytes_of_each_lexeme() {
    let mut lexer = Lexer::new(SliceBytes::new(b"\"abcdef\" 12")).with_capture(4);
    let first = lexer.next().expect("slice").expect("a lexeme");
    assert_eq!((first.start, first.end), (0, 8));
    assert_eq!(lexer.captured(), b"\"abc");
    lexer.next().expect("slice").expect("a lexeme");
    assert_eq!(lexer.captured(), b"12");
}
