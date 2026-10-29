use super::*;
use crate::Viewer;
use crate::test_support::source;
use crate::types::Settings;

use crate::types::TokenClass::{Invalid, Key, Number, Punctuation as P, String as Str};

/// The tokens of one line on its own, as the text each covers.
fn lex(line: &str, sep: u8) -> Vec<(&str, TokenClass)> {
    let tokens = LineLexer::new(sep, LineState::default()).line(line.as_bytes(), false, false);
    spans(line, &tokens)
}

fn spans<'a>(line: &'a str, tokens: &[Token]) -> Vec<(&'a str, TokenClass)> {
    tokens
        .iter()
        .map(|token| {
            let start = token.start as usize;
            (&line[start..start + token.len as usize], token.class)
        })
        .collect()
}

/// The tokens of every line of `text`, opened with the default settings.
fn document(text: &str) -> Vec<Vec<Token>> {
    let mut viewer = Viewer::open(source(text), Settings::default())
        .expect("opens")
        .0;
    viewer.tokens(0, 100).expect("tokens")
}

#[test]
fn every_column_takes_its_class_in_turn() {
    let line = "a,b,c,d,e,f,g,h,i,j";
    let tokens = lex(line, b',');
    let fields: Vec<TokenClass> = tokens
        .iter()
        .filter(|(_, class)| *class != P)
        .map(|(_, class)| *class)
        .collect();
    let want: Vec<TokenClass> = (0..10).map(column_class).collect();
    assert_eq!(fields, want);
    // Column 9 is `key` again.
    assert_eq!(fields[8], Key);
    assert_eq!(COLUMN_CLASSES.len(), 8);
    assert!(
        tokens
            .iter()
            .filter(|(_, class)| *class == P)
            .all(|(text, _)| *text == ",")
    );
}

#[test]
fn all_four_separators() {
    for sep in *b",;\t|" {
        let separator = char::from(sep).to_string();
        let line = ["x", "y", "z"].join(&separator);
        assert_eq!(
            lex(&line, sep),
            vec![
                ("x", Key),
                (separator.as_str(), P),
                ("y", Str),
                (separator.as_str(), P),
                ("z", Number)
            ],
            "{sep}"
        );
    }
    // Another separator is text.
    assert_eq!(lex("a;b", b','), vec![("a;b", Key)]);
}

#[test]
fn quoted_fields_with_doubled_quotes_and_separators_keep_their_column() {
    assert_eq!(
        lex("\"a \"\"b\"\", c\",d", b','),
        vec![("\"a \"\"b\"\", c\"", Key), (",", P), ("d", Str)]
    );
}

#[test]
fn empty_fields_get_no_token_but_count_as_columns() {
    assert_eq!(
        lex(",a,,b,", b','),
        vec![
            (",", P),
            ("a", Str),
            (",", P),
            (",", P),
            ("b", TokenClass::Keyword),
            (",", P)
        ]
    );
}

#[test]
fn text_after_a_closing_quote_is_invalid() {
    assert_eq!(
        lex("\"ok\"x,y,\"z\"", b','),
        vec![
            ("\"ok\"x", Invalid),
            (",", P),
            ("y", Str),
            (",", P),
            ("\"z\"", Number)
        ]
    );
}

#[test]
fn a_line_break_inside_quotes_keeps_the_column_on_the_next_line() {
    let text = "id,text,n\n1,\"two\nlines, here\",3\n4,x,5\n";
    let tokens = document(text);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(tokens.len(), 4);
    assert_eq!(
        spans(lines[1], &tokens[1]),
        vec![("1", Key), (",", P), ("\"two", Str)]
    );
    assert_eq!(
        spans(lines[2], &tokens[2]),
        vec![("lines, here\"", Str), (",", P), ("3", Number)]
    );
    assert_eq!(spans(lines[3], &tokens[3])[0], ("4", Key));
    // Asked for alone, the line is lexed from the checkpoint before it and gets the same.
    let mut viewer = Viewer::open(source(text), Settings::default())
        .expect("opens")
        .0;
    assert_eq!(
        viewer.tokens(2, 1).expect("tokens"),
        vec![tokens[2].clone()]
    );
}

#[test]
fn a_quote_never_closed_is_invalid_to_the_end() {
    let text = "a,b\nc,\"open\nmore, text\n";
    let tokens = document(text);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(spans(lines[0], &tokens[0])[0], ("a", Key));
    assert_eq!(
        spans(lines[1], &tokens[1]),
        vec![("c", Key), (",", P), ("\"open", Invalid)]
    );
    assert_eq!(spans(lines[2], &tokens[2]), vec![("more, text", Invalid)]);
}

#[test]
fn crlf_and_a_byte_order_mark_are_not_part_of_the_tokens() {
    let tokens = document("\u{feff}a;b\r\n\r\nc;\"d\"\r\n");
    assert_eq!(tokens.len(), 3);
    let (first, blank, last) = (&tokens[0], &tokens[1], &tokens[2]);
    assert_eq!(first.first().map(|token| token.start), Some(0));
    assert_eq!(spans("a;b", first), vec![("a", Key), (";", P), ("b", Str)]);
    assert!(blank.is_empty());
    assert_eq!(
        spans("c;\"d\"", last),
        vec![("c", Key), (";", P), ("\"d\"", Str)]
    );
}

#[test]
fn a_line_gets_at_most_4096_tokens() {
    let line = vec!["x"; 5000].join(",");
    let tokens = LineLexer::new(b',', LineState::default()).line(line.as_bytes(), false, false);
    assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE);
    assert_eq!(tokens.last().map(|token| token.class), Some(P));
}

#[test]
fn a_line_past_the_cap_leaves_its_last_field_uncoloured() {
    let long = "y".repeat(MAX_LINE_BYTES + 10);
    let text = format!("a,{long},z\nq,\"r\"\n");
    let tokens = document(&text);
    assert_eq!(spans("a,", &tokens[0]), vec![("a", Key), (",", P)]);
    assert_eq!(
        spans("q,\"r\"", &tokens[1]),
        vec![("q", Key), (",", P), ("\"r\"", Str)]
    );
    // Cut by the cap inside a field, the state still carries over without the next checkpoint.
    let mut lexer = LineLexer::new(b',', LineState::default());
    let head = lexer.line(b"a,\"yy", true, false);
    assert_eq!(head.len(), 2);
    lexer.skip(b"y, still quoted");
    assert_eq!(
        lexer.end_line(),
        LineState {
            quoted: true,
            column: 1
        }
    );
}
