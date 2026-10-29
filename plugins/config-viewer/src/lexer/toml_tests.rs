use crate::lexer::{LineState, Multi, TomlState};
use crate::test_support::pieces;
use crate::types::Format;
use crate::types::TokenClass::{
    self, Comment, Invalid, Key, Keyword, Literal, Number, Punctuation, String as Str,
};

fn toml(line: &str, state: LineState) -> (Vec<(&str, TokenClass)>, LineState) {
    pieces(line, state, Format::Toml)
}

fn clear(line: &str) -> Vec<(&str, TokenClass)> {
    let (tokens, next) = toml(line, LineState::Clear);
    assert_eq!(next, LineState::Clear, "{line}");
    tokens
}

#[test]
fn tables_and_arrays_of_tables() {
    assert_eq!(clear("[package]"), [("[package]", Keyword)]);
    assert_eq!(clear("  [[bin]]"), [("[[bin]]", Keyword)]);
    assert_eq!(
        clear("[a.\"b.c\"] # x"),
        [("[a.\"b.c\"]", Keyword), ("# x", Comment)]
    );
    assert_eq!(clear("[[broken]"), [("[[broken]", Invalid)]);
    assert_eq!(clear("[a] b"), [("[a]", Keyword), ("b", Invalid)]);
}

#[test]
fn keys_bare_quoted_and_dotted() {
    assert_eq!(
        clear("name = \"rc\""),
        [("name", Key), ("=", Punctuation), ("\"rc\"", Str)]
    );
    assert_eq!(
        clear("a.b.\"c\" = 1_000"),
        [
            ("a", Key),
            (".", Punctuation),
            ("b", Key),
            (".", Punctuation),
            ("\"c\"", Key),
            ("=", Punctuation),
            ("1_000", Number)
        ]
    );
    assert_eq!(
        clear("'lit key' = 'C:\\path'"),
        [("'lit key'", Key), ("=", Punctuation), ("'C:\\path'", Str)]
    );
    assert_eq!(
        clear("\u{e4} = 1"),
        [("\u{e4}", Invalid), ("=", Punctuation), ("1", Number)]
    );
}

#[test]
fn values_numbers_booleans_dates() {
    let value = |line: &'static str| clear(line)[2];
    assert_eq!(value("h = 0xDEAD_beef"), ("0xDEAD_beef", Number));
    assert_eq!(value("f = -3.5e+2"), ("-3.5e+2", Number));
    assert_eq!(value("i = +inf"), ("+inf", Number));
    assert_eq!(value("b = 1__0"), ("1__0", Invalid));
    assert_eq!(value("t = true"), ("true", Literal));
    assert_eq!(
        value("d = 1979-05-27T07:32:00Z"),
        ("1979-05-27T07:32:00Z", Literal)
    );
    assert_eq!(
        value("d = 1979-05-27 07:32:00.999-07:00"),
        ("1979-05-27 07:32:00.999-07:00", Literal)
    );
    assert_eq!(value("t = 07:32:00"), ("07:32:00", Literal));
    assert_eq!(value("d = 1979-05-27"), ("1979-05-27", Literal));
    assert_eq!(value("x = nope"), ("nope", Invalid));
    assert_eq!(value("s = \"a\\\"b\""), ("\"a\\\"b\"", Str));
}

#[test]
fn multi_line_strings_of_both_kinds() {
    let (tokens, next) = toml("s = \"\"\"first", LineState::Clear);
    assert_eq!(
        tokens,
        [("s", Key), ("=", Punctuation), ("\"\"\"first", Str)]
    );
    let open = LineState::Toml(TomlState {
        string: Multi::Basic,
        ..TomlState::default()
    });
    assert_eq!(next, open);
    // A quote, and an escaped one, inside: still open.
    let (tokens, next) = toml("say \"hi\" \\\"\"\"", next);
    assert_eq!(tokens, [("say \"hi\" \\\"\"\"", Str)]);
    assert_eq!(next, open);
    let (tokens, next) = toml("end\"\"\" # c", next);
    assert_eq!(tokens, [("end\"\"\"", Str), ("# c", Comment)]);
    assert_eq!(next, LineState::Clear);

    assert_eq!(
        clear("s = '''a'b'''"),
        [("s", Key), ("=", Punctuation), ("'''a'b'''", Str)]
    );
    // Up to two quotes before the closing three belong to the string.
    assert_eq!(
        clear("s = \"\"\"x\"\"\"\"\""),
        [("s", Key), ("=", Punctuation), ("\"\"\"x\"\"\"\"\"", Str)]
    );
    let (_, next) = toml("p = '''", LineState::Clear);
    let (tokens, next) = toml("[not a header]", next);
    assert_eq!(tokens, [("[not a header]", Str)]);
    assert_eq!(toml("'''", next).1, LineState::Clear);
}

#[test]
fn inline_tables_and_arrays_over_lines() {
    assert_eq!(
        clear("p = { x = 1, y = \"two\" }"),
        [
            ("p", Key),
            ("=", Punctuation),
            ("{", Punctuation),
            ("x", Key),
            ("=", Punctuation),
            ("1", Number),
            (",", Punctuation),
            ("y", Key),
            ("=", Punctuation),
            ("\"two\"", Str),
            ("}", Punctuation)
        ]
    );
    let (tokens, next) = toml("deps = [", LineState::Clear);
    assert_eq!(
        tokens,
        [("deps", Key), ("=", Punctuation), ("[", Punctuation)]
    );
    assert!(matches!(next, LineState::Toml(state) if state.depth == 1));
    // In an array a string is a value, never a key, and `[` no header.
    let (tokens, next) = toml("  \"a\", # c", next);
    assert_eq!(
        tokens,
        [("\"a\"", Str), (",", Punctuation), ("# c", Comment)]
    );
    let (tokens, next) = toml("  [1, 2],", next);
    assert_eq!(
        tokens,
        [
            ("[", Punctuation),
            ("1", Number),
            (",", Punctuation),
            ("2", Number),
            ("]", Punctuation),
            (",", Punctuation)
        ]
    );
    let (tokens, next) = toml("  { k = 1 },", next);
    assert_eq!(
        tokens,
        [
            ("{", Punctuation),
            ("k", Key),
            ("=", Punctuation),
            ("1", Number),
            ("}", Punctuation),
            (",", Punctuation)
        ]
    );
    let (tokens, next) = toml("]", next);
    assert_eq!(tokens, [("]", Punctuation)]);
    assert_eq!(next, LineState::Clear);
    assert_eq!(
        clear("x = ]"),
        [("x", Key), ("=", Punctuation), ("]", Invalid)]
    );
}
