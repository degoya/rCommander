use super::*;
use crate::dialect::Dialect;
use crate::reader::SliceBytes;

fn lex(text: &str) -> (Vec<Lexeme>, Vec<(u64, u64)>) {
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes()));
    let mut lexemes = Vec::new();
    let mut starts = Vec::new();
    while let Some(lexeme) = lexer
        .next_with(&mut |line, offset, _| starts.push((line, offset)))
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

fn kinds(lexer: &mut Lexer<SliceBytes<'_>>) -> Vec<(Kind, u64, u64)> {
    let mut out = Vec::new();
    while let Some(lexeme) = lexer.next().expect("a slice cannot fail") {
        out.push((lexeme.kind, lexeme.start, lexeme.end));
    }
    out
}

#[test]
fn over_the_whole_stream_comments_are_whitespace_and_name_the_lines_inside_them() {
    let text = "// a\n[1, /* b\nc */ 2]";
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes())).dialect(Dialect::Jsonc);
    let mut lexemes = Vec::new();
    let mut starts = Vec::new();
    while let Some(lexeme) = lexer
        .next_with(&mut |line, offset, inside| starts.push((line, offset, inside)))
        .expect("a slice cannot fail")
    {
        lexemes.push(lexeme.kind);
    }
    assert_eq!(
        lexemes,
        vec![
            Kind::BeginArray,
            Kind::Number,
            Kind::Comma,
            Kind::Number,
            Kind::EndArray,
        ]
    );
    assert_eq!(starts, vec![(1, 5, false), (2, 14, true)]);
    // In JSON a slash is no comment.
    let mut json = Lexer::new(SliceBytes::new(b"// a"));
    assert_eq!(
        kinds(&mut json)[0],
        (Kind::Invalid(Problem::Unexpected), 0, 2)
    );
}

#[test]
fn per_line_a_comment_is_a_lexeme_and_a_block_runs_on_past_the_line() {
    let mut first = Lexer::new(SliceBytes::new(b"1 /* open"))
        .dialect(Dialect::Json5)
        .per_line(false);
    assert_eq!(
        kinds(&mut first),
        vec![(Kind::Number, 0, 1), (Kind::Comment, 2, 9)]
    );
    assert!(first.in_block_comment());
    let mut next = Lexer::new(SliceBytes::new(b"  still */ 2 // end"))
        .dialect(Dialect::Json5)
        .per_line(true);
    assert_eq!(
        kinds(&mut next),
        vec![
            (Kind::Comment, 0, 10),
            (Kind::Number, 11, 12),
            (Kind::Comment, 13, 19)
        ]
    );
    assert!(!next.in_block_comment());
}

#[test]
fn an_unterminated_block_comment_is_invalid_over_the_whole_stream() {
    let mut lexer = Lexer::new(SliceBytes::new(b"[1 /* x\ny")).dialect(Dialect::Jsonc);
    let all = kinds(&mut lexer);
    assert_eq!(
        all.last(),
        Some(&(Kind::Invalid(Problem::UnterminatedComment), 3, 9))
    );
}

#[test]
fn json5_numbers_strings_and_names() {
    let text = "0x1F +1 .5 5. -Infinity NaN 'it\\'s' \"\\x41\" $id true 01 0x 1e";
    let mut lexer = Lexer::new(SliceBytes::new(text.as_bytes())).dialect(Dialect::Json5);
    let got: Vec<Kind> = kinds(&mut lexer).iter().map(|lexeme| lexeme.0).collect();
    assert_eq!(
        got,
        vec![
            Kind::Number,
            Kind::Number,
            Kind::Number,
            Kind::Number,
            Kind::Number,
            Kind::Number,
            Kind::String,
            Kind::String,
            Kind::Identifier,
            Kind::Literal,
            Kind::Invalid(Problem::BadNumber),
            Kind::Invalid(Problem::BadNumber),
            Kind::Invalid(Problem::BadNumber),
        ]
    );
    // The same in JSON: none of JSON5's own.
    let mut json = Lexer::new(SliceBytes::new(b"+1 'a' $id"));
    let got: Vec<Kind> = kinds(&mut json).iter().map(|lexeme| lexeme.0).collect();
    assert!(got.iter().all(|kind| matches!(kind, Kind::Invalid(_))));
}
