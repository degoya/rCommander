use super::*;
use crate::types::TokenClass::{
    Comment, Invalid, Key, Keyword, Literal, Punctuation, String as Str, TypeName,
};

/// The text and class of every token of `line`, lexed from `mode`.
fn spans_in(line: &str, mode: Mode) -> Vec<(&str, TokenClass)> {
    spans_of(line, tokenize_line(line.as_bytes(), mode, false).0)
}

fn spans(line: &str) -> Vec<(&str, TokenClass)> {
    spans_in(line, Mode::Content)
}

fn spans_of(line: &str, tokens: Vec<Token>) -> Vec<(&str, TokenClass)> {
    assert!(
        tokens
            .windows(2)
            .all(|pair| pair[0].start + pair[0].len <= pair[1].start),
        "sorted, not overlapping"
    );
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

/// The spans of every line of `text`, each line lexed from the mode the one before ended in.
fn lines(text: &str) -> Vec<Vec<(&str, TokenClass)>> {
    let mut mode = Mode::Content;
    text.split('\n')
        .map(|line| {
            let (tokens, next) = tokenize_line(line.as_bytes(), mode, false);
            mode = next;
            spans_of(line, tokens)
        })
        .collect()
}

#[test]
fn elements_and_attributes_with_both_quotes() {
    assert_eq!(
        spans(r#"<item id="7" name='x y'>text</item>"#),
        vec![
            ("<", Punctuation),
            ("item", TypeName),
            ("id", Key),
            ("=", Punctuation),
            ("\"7\"", Str),
            ("name", Key),
            ("=", Punctuation),
            ("'x y'", Str),
            (">", Punctuation),
            ("</", Punctuation),
            ("item", TypeName),
            (">", Punctuation),
        ]
    );
    assert_eq!(
        spans("<empty />"),
        vec![("<", Punctuation), ("empty", TypeName), ("/>", Punctuation)]
    );
    // A quote of the other kind does not end a value.
    assert_eq!(spans(r#"<a b="it's">"#)[4], (r#""it's""#, Str));
}

#[test]
fn namespaced_names_are_one_name() {
    assert_eq!(
        spans(r#"<xs:element xmlns:xs="urn:x" xs:type="t"/>"#),
        vec![
            ("<", Punctuation),
            ("xs:element", TypeName),
            ("xmlns:xs", Key),
            ("=", Punctuation),
            ("\"urn:x\"", Str),
            ("xs:type", Key),
            ("=", Punctuation),
            ("\"t\"", Str),
            ("/>", Punctuation),
        ]
    );
}

#[test]
fn a_comment_over_lines_is_a_comment_on_each() {
    assert_eq!(
        lines("<a><!-- one\n  two <b> & \"\nthree --><c/></a>"),
        vec![
            vec![
                ("<", Punctuation),
                ("a", TypeName),
                (">", Punctuation),
                ("<!-- one", Comment)
            ],
            vec![("  two <b> & \"", Comment)],
            vec![
                ("three -->", Comment),
                ("<", Punctuation),
                ("c", TypeName),
                ("/>", Punctuation),
                ("</", Punctuation),
                ("a", TypeName),
                (">", Punctuation),
            ],
        ]
    );
}

#[test]
fn cdata_over_lines_is_a_string_between_punctuation() {
    assert_eq!(
        lines("<s><![CDATA[if (a < b)\n  && c]]>]]></s>"),
        vec![
            vec![
                ("<", Punctuation),
                ("s", TypeName),
                (">", Punctuation),
                ("<![CDATA[", Punctuation),
                ("if (a < b)", Str),
            ],
            vec![
                ("  && c", Str),
                ("]]>", Punctuation),
                // Text after the section: no token for `]]>` in text, no colour.
                ("</", Punctuation),
                ("s", TypeName),
                (">", Punctuation),
            ],
        ]
    );
    assert_eq!(
        spans("<![CDATA[]]>"),
        vec![("<![CDATA[", Punctuation), ("]]>", Punctuation)]
    );
}

#[test]
fn processing_instructions_and_doctype_are_keywords() {
    assert_eq!(
        spans(r#"<?xml version="1.0" encoding="UTF-8"?>"#),
        vec![(r#"<?xml version="1.0" encoding="UTF-8"?>"#, Keyword)]
    );
    assert_eq!(
        lines("<!DOCTYPE note [\n  <!ENTITY w \"a > b\">\n]><note/>"),
        vec![
            vec![("<!DOCTYPE note [", Keyword)],
            vec![("  <!ENTITY w \"a > b\">", Keyword)],
            vec![
                ("]>", Keyword),
                ("<", Punctuation),
                ("note", TypeName),
                ("/>", Punctuation)
            ],
        ]
    );
}

#[test]
fn entities_in_text_and_values_are_literals() {
    assert_eq!(
        spans(r#"<a t="x &amp; &#233;">1 &lt; 2 &#x1F600;</a>"#),
        vec![
            ("<", Punctuation),
            ("a", TypeName),
            ("t", Key),
            ("=", Punctuation),
            ("\"x ", Str),
            ("&amp;", Literal),
            (" ", Str),
            ("&#233;", Literal),
            ("\"", Str),
            (">", Punctuation),
            ("&lt;", Literal),
            ("&#x1F600;", Literal),
            ("</", Punctuation),
            ("a", TypeName),
            (">", Punctuation),
        ]
    );
    for bad in ["&", "&amp", "&#;", "&#x;", "&#12a;"] {
        assert_eq!(spans(bad)[0].1, Invalid, "{bad}");
    }
}

#[test]
fn characters_xml_has_no_place_for_are_invalid() {
    assert_eq!(
        spans("<a @x=\"1\" b>< </ a>"),
        vec![
            ("<", Punctuation),
            ("a", TypeName),
            ("@x", Invalid),
            ("=", Punctuation),
            ("\"1\"", Str),
            ("b", Key),
            (">", Punctuation),
            ("<", Invalid),
            ("</", Invalid),
        ]
    );
    assert_eq!(spans("<!x-y>")[0], ("<!x-y>", Keyword));
    assert_eq!(spans("<!-x")[0], ("<!-", Invalid));
    assert_eq!(spans("<![CDAT")[0], ("<![CDAT", Invalid));
}

#[test]
fn unicode_spans_sit_on_character_boundaries() {
    assert_eq!(
        spans("<gr\u{fc}\u{df}e \u{e9}t\u{e9}=\"\u{65e5}\u{672c}\"/>"),
        vec![
            ("<", Punctuation),
            ("gr\u{fc}\u{df}e", TypeName),
            ("\u{e9}t\u{e9}", Key),
            ("=", Punctuation),
            ("\"\u{65e5}\u{672c}\"", Str),
            ("/>", Punctuation),
        ]
    );
}

#[test]
fn a_line_inside_a_tag_or_value_starts_in_it() {
    assert_eq!(
        spans_in("  b=\"1\">text", Mode::Tag),
        vec![
            ("b", Key),
            ("=", Punctuation),
            ("\"1\"", Str),
            (">", Punctuation)
        ]
    );
    assert_eq!(
        spans_in("rest\" c='2'/>", Mode::Value(b'"')),
        vec![
            ("rest\"", Str),
            ("c", Key),
            ("=", Punctuation),
            ("'2'", Str),
            ("/>", Punctuation)
        ]
    );
    assert_eq!(end_mode(b"<a b=\"open", Mode::Content), Mode::Value(b'"'));
    assert_eq!(end_mode(b"<a", Mode::Content), Mode::Tag);
    assert_eq!(end_mode(b"<a>", Mode::Content), Mode::Content);
}

#[test]
fn a_line_keeps_at_most_its_token_budget() {
    let line = "<a/>".repeat(MAX_TOKENS_PER_LINE);
    let (tokens, mode) = tokenize_line(line.as_bytes(), Mode::Content, false);
    assert_eq!(tokens.len(), MAX_TOKENS_PER_LINE);
    assert_eq!(mode, Mode::Content);
    // The mode is the line's end's even past the budget.
    let open = format!("{line}<!-- open");
    assert_eq!(
        tokenize_line(open.as_bytes(), Mode::Content, false).1,
        Mode::Comment
    );
}

#[test]
fn a_capped_line_drops_the_lexeme_the_cap_may_have_cut() {
    let line = "<a b=\"long";
    assert_eq!(
        spans_of(line, tokenize_line(line.as_bytes(), Mode::Content, true).0),
        vec![
            ("<", Punctuation),
            ("a", TypeName),
            ("b", Key),
            ("=", Punctuation)
        ]
    );
}

#[test]
fn text_and_blank_lines_have_no_tokens() {
    assert!(spans("").is_empty());
    assert!(spans(" \t ").is_empty());
    assert!(spans("just text > here").is_empty());
}
