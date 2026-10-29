use crate::lexer::LineState;
use crate::test_support::pieces;
use crate::types::Format;
use crate::types::TokenClass::{
    self, Comment, Invalid, Key, Keyword, Literal, Number, Punctuation, String as Str,
};

fn ini(line: &str) -> Vec<(&str, TokenClass)> {
    pieces(line, LineState::Clear, Format::Ini).0
}

fn lines(line: &str) -> Vec<(&str, TokenClass)> {
    pieces(line, LineState::Clear, Format::Lines).0
}

#[test]
fn sections_and_comments() {
    assert_eq!(ini("[General]"), [("[General]", Keyword)]);
    assert_eq!(
        ini("  [remote \"origin\"] ; c"),
        [("[remote \"origin\"]", Keyword), ("; c", Comment)]
    );
    assert_eq!(ini("[*.{js,py}]"), [("[*.{js,py}]", Keyword)]);
    assert_eq!(ini("[*.[ch]]"), [("[*.[ch]]", Keyword)]);
    assert_eq!(ini("[a] x"), [("[a]", Keyword), ("x", Invalid)]);
    assert_eq!(ini("; comment"), [("; comment", Comment)]);
    assert_eq!(ini("  # comment"), [("# comment", Comment)]);
    assert_eq!(ini("[broken"), [("[broken", Invalid)]);
    assert!(ini("   ").is_empty());
}

#[test]
fn keys_and_their_values() {
    assert_eq!(
        ini("name = J\u{fc}rgen"),
        [("name", Key), ("=", Punctuation), ("J\u{fc}rgen", Str)]
    );
    assert_eq!(
        ini("port=8080"),
        [("port", Key), ("=", Punctuation), ("8080", Number)]
    );
    assert_eq!(
        ini("enabled = true ; inline"),
        [
            ("enabled", Key),
            ("=", Punctuation),
            ("true", Literal),
            ("; inline", Comment)
        ]
    );
    assert_eq!(
        ini("path = C:\\Program Files\\x"),
        [
            ("path", Key),
            ("=", Punctuation),
            ("C:\\Program Files\\x", Str)
        ]
    );
    assert_eq!(
        ini("url: http://x"),
        [("url", Key), (":", Punctuation), ("http://x", Str)]
    );
    assert_eq!(
        ini("key = a#b"),
        [("key", Key), ("=", Punctuation), ("a#b", Str)]
    );
    assert_eq!(
        ini("quoted = \"a \\\"b\\\" ; c\" ; d"),
        [
            ("quoted", Key),
            ("=", Punctuation),
            ("\"a \\\"b\\\" ; c\"", Str),
            ("; d", Comment)
        ]
    );
    assert_eq!(ini("bare"), [("bare", Key)]);
    assert_eq!(ini("bare # c"), [("bare", Key), ("# c", Comment)]);
}

#[test]
fn an_inf_list_is_split_by_its_commas() {
    assert_eq!(
        ini("CopyFiles = a.sys, 1,, \"x y\""),
        [
            ("CopyFiles", Key),
            ("=", Punctuation),
            ("a.sys", Str),
            (",", Punctuation),
            ("1", Number),
            (",", Punctuation),
            (",", Punctuation),
            ("\"x y\"", Str)
        ]
    );
    assert_eq!(
        ini("open = \"unclosed"),
        [("open", Key), ("=", Punctuation), ("\"unclosed", Invalid)]
    );
}

#[test]
fn git_lines_are_a_pattern_and_attributes() {
    assert_eq!(lines("# comment"), [("# comment", Comment)]);
    assert_eq!(lines("!important.log"), [("!important.log", Key)]);
    assert_eq!(
        lines("*.txt text eol=lf -diff"),
        [
            ("*.txt", Key),
            ("text", Str),
            ("eol", Str),
            ("=", Punctuation),
            ("lf", Str),
            ("-", Punctuation),
            ("diff", Str)
        ]
    );
    assert_eq!(
        lines("path\\ with\\ space/"),
        [("path\\ with\\ space/", Key)]
    );
    assert!(lines("").is_empty());
}
