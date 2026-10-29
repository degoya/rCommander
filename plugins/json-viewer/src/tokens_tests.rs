use super::*;
use crate::types::TokenClass::{Invalid, Key, Literal, Number, Punctuation};

/// The text and class of every token of `line`.
fn spans(line: &str) -> Vec<(&str, TokenClass)> {
    spans_of(line, tokenize_line(line.as_bytes(), false, false))
}

fn spans_of(line: &str, tokens: Vec<Token>) -> Vec<(&str, TokenClass)> {
    tokens
        .iter()
        .map(|token| {
            let start = token.start as usize;
            let end = start + token.len as usize;
            assert!(line.is_char_boundary(start) && line.is_char_boundary(end));
            (&line[start..end], token.class)
        })
        .collect()
}

#[test]
fn a_string_before_a_colon_is_a_key_and_after_it_a_value() {
    assert_eq!(
        spans(r#"{"name": "value", "list": ["a", "b"]}"#),
        vec![
            ("{", Punctuation),
            ("\"name\"", Key),
            (":", Punctuation),
            ("\"value\"", TokenClass::String),
            (",", Punctuation),
            ("\"list\"", Key),
            (":", Punctuation),
            ("[", Punctuation),
            ("\"a\"", TokenClass::String),
            (",", Punctuation),
            ("\"b\"", TokenClass::String),
            ("]", Punctuation),
            ("}", Punctuation),
        ]
    );
    assert_eq!(
        spans("  \"spaced\"   :1"),
        vec![("\"spaced\"", Key), (":", Punctuation), ("1", Number)]
    );
}

#[test]
fn a_colon_on_the_next_line_makes_the_last_string_a_key() {
    let line = "  \"alone\"";
    assert_eq!(
        spans_of(line, tokenize_line(line.as_bytes(), false, true)),
        vec![("\"alone\"", Key)]
    );
    assert_eq!(spans(line), vec![("\"alone\"", TokenClass::String)]);
    assert!(colon_follows([b"   ".as_slice(), b"\t: 1".as_slice()]));
    assert!(!colon_follows([b" 1".as_slice(), b":".as_slice()]));
    assert!(!colon_follows(std::iter::empty::<&[u8]>()));
}

#[test]
fn escapes_stay_inside_their_string() {
    let line = r#"["a\"b\\c\/\b\f\n\r\t\u00e9\uD83D\uDE00", "\x", "\u12g4"]"#;
    assert_eq!(
        spans(line),
        vec![
            ("[", Punctuation),
            (
                r#""a\"b\\c\/\b\f\n\r\t\u00e9\uD83D\uDE00""#,
                TokenClass::String
            ),
            (",", Punctuation),
            (r#""\x""#, Invalid),
            (",", Punctuation),
            (r#""\u12g4""#, Invalid),
            ("]", Punctuation),
        ]
    );
}

#[test]
fn unicode_spans_sit_on_character_boundaries() {
    let line = "{\"gr\u{fc}\u{df}e\": \"\u{65e5}\u{672c}\u{1F600}\", \u{e9}: 1}";
    assert_eq!(
        spans(line),
        vec![
            ("{", Punctuation),
            ("\"gr\u{fc}\u{df}e\"", Key),
            (":", Punctuation),
            ("\"\u{65e5}\u{672c}\u{1F600}\"", TokenClass::String),
            (",", Punctuation),
            ("\u{e9}", Invalid),
            (":", Punctuation),
            ("1", Number),
            ("}", Punctuation),
        ]
    );
}

#[test]
fn numbers_follow_the_json_grammar() {
    for number in [
        "0", "-0", "-1", "2.5", "1e10", "-2.5E-3", "6.02e+23", "10", "0.0",
    ] {
        assert_eq!(spans(number), vec![(number, Number)], "{number}");
    }
    for bad in [
        "01", "1.", "-", "1e", "1e+", ".5", "+1", "1.2.3", "12abc", "--1", "0x1F",
    ] {
        assert_eq!(spans(bad), vec![(bad, Invalid)], "{bad}");
    }
    assert_eq!(
        spans("[1,-2]"),
        vec![
            ("[", Punctuation),
            ("1", Number),
            (",", Punctuation),
            ("-2", Number),
            ("]", Punctuation)
        ]
    );
}

#[test]
fn only_the_three_literals_are_literals() {
    assert_eq!(
        spans("[true, false, null]")
            .into_iter()
            .filter(|(_, class)| *class == Literal)
            .count(),
        3
    );
    for bad in ["True", "nul", "nullx", "undefined", "NaN", "Infinity"] {
        assert_eq!(spans(bad), vec![(bad, Invalid)], "{bad}");
    }
}

#[test]
fn characters_json_has_no_place_for_are_invalid() {
    assert_eq!(
        spans("{'a': @#}"),
        vec![
            ("{", Punctuation),
            ("'a'", Invalid),
            (":", Punctuation),
            ("@#", Invalid),
            ("}", Punctuation),
        ]
    );
    assert_eq!(spans("\"open"), vec![("\"open", Invalid)]);
    assert_eq!(spans("\"tab\there\""), vec![("\"tab\there\"", Invalid)]);
}

#[test]
fn empty_and_blank_lines_have_no_tokens() {
    assert!(spans("").is_empty());
    assert!(spans(" \t ").is_empty());
    assert!(is_blank(b" \t\r"));
    assert!(!is_blank(b" x"));
}

#[test]
fn a_line_keeps_at_most_its_token_budget() {
    let line = "1,".repeat(MAX_TOKENS_PER_LINE);
    let tokens = tokenize_line(line.as_bytes(), false, false);
    assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE);
    assert!(
        tokens
            .windows(2)
            .all(|pair| pair[0].start + pair[0].len <= pair[1].start)
    );
}

#[test]
fn a_capped_line_drops_the_token_the_cap_may_have_cut() {
    let line = "[1, \"long";
    assert_eq!(
        spans_of(line, tokenize_line(line.as_bytes(), true, true)),
        vec![("[", Punctuation), ("1", Number), (",", Punctuation)]
    );
    let line = "[1, 23";
    assert_eq!(
        spans_of(line, tokenize_line(line.as_bytes(), true, false)).len(),
        3
    );
}
