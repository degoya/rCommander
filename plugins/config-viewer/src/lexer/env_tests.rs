use crate::lexer::LineState;
use crate::test_support::pieces;
use crate::types::Format;
use crate::types::TokenClass::{
    self, Comment, Invalid, Key, Keyword, Literal, Number, Punctuation, String as Str,
};

fn env(line: &str, state: LineState) -> (Vec<(&str, TokenClass)>, LineState) {
    pieces(line, state, Format::Env)
}

fn props(line: &str, state: LineState) -> (Vec<(&str, TokenClass)>, LineState) {
    pieces(line, state, Format::Properties)
}

fn env_clear(line: &str) -> Vec<(&str, TokenClass)> {
    env(line, LineState::Clear).0
}

fn props_clear(line: &str) -> Vec<(&str, TokenClass)> {
    props(line, LineState::Clear).0
}

#[test]
fn env_lines() {
    assert_eq!(env_clear("# c"), [("# c", Comment)]);
    assert_eq!(
        env_clear("export DB_HOST=localhost"),
        [
            ("export", Keyword),
            ("DB_HOST", Key),
            ("=", Punctuation),
            ("localhost", Str)
        ]
    );
    assert_eq!(
        env_clear("PORT=5432 # db"),
        [
            ("PORT", Key),
            ("=", Punctuation),
            ("5432", Number),
            ("# db", Comment)
        ]
    );
    assert_eq!(
        env_clear("DEBUG=true"),
        [("DEBUG", Key), ("=", Punctuation), ("true", Literal)]
    );
    assert_eq!(
        env_clear("SECRET=\"a b # not a comment\""),
        [
            ("SECRET", Key),
            ("=", Punctuation),
            ("\"a b # not a comment\"", Str)
        ]
    );
    assert_eq!(
        env_clear("NAME='x' # c"),
        [
            ("NAME", Key),
            ("=", Punctuation),
            ("'x'", Str),
            ("# c", Comment)
        ]
    );
    assert_eq!(
        env_clear("URL=http://x#frag"),
        [("URL", Key), ("=", Punctuation), ("http://x#frag", Str)]
    );
    assert_eq!(
        env_clear("export ONLY"),
        [("export", Keyword), ("ONLY", Key)]
    );
    assert_eq!(env_clear("=nokey"), [("=nokey", Invalid)]);
    assert_eq!(
        env_clear("export=1"),
        [("export", Key), ("=", Punctuation), ("1", Number)]
    );
}

#[test]
fn a_quoted_env_value_goes_on_over_lines() {
    let (tokens, next) = env("KEY=\"line one", LineState::Clear);
    assert_eq!(
        tokens,
        [("KEY", Key), ("=", Punctuation), ("\"line one", Str)]
    );
    assert_eq!(next, LineState::Quoted(b'"'));
    let (tokens, next) = env("line two", next);
    assert_eq!(tokens, [("line two", Str)]);
    assert_eq!(next, LineState::Quoted(b'"'));
    let (tokens, next) = env("end\" # c", next);
    assert_eq!(tokens, [("end\"", Str), ("# c", Comment)]);
    assert_eq!(next, LineState::Clear);
}

#[test]
fn properties_lines() {
    assert_eq!(props_clear("# c"), [("# c", Comment)]);
    assert_eq!(props_clear("! c"), [("! c", Comment)]);
    assert_eq!(
        props_clear("key=value"),
        [("key", Key), ("=", Punctuation), ("value", Str)]
    );
    assert_eq!(
        props_clear("key: 42"),
        [("key", Key), (":", Punctuation), ("42", Number)]
    );
    assert_eq!(
        props_clear("key value with spaces"),
        [("key", Key), ("value with spaces", Str)]
    );
    assert_eq!(
        props_clear("my\\ key = v"),
        [("my\\ key", Key), ("=", Punctuation), ("v", Str)]
    );
}

#[test]
fn a_backslash_continues_a_properties_value() {
    let (tokens, next) = props("list = a,\\", LineState::Clear);
    assert_eq!(tokens, [("list", Key), ("=", Punctuation), ("a,\\", Str)]);
    assert_eq!(next, LineState::Continued);
    let (tokens, next) = props("    # b,\\", next);
    assert_eq!(tokens, [("# b,\\", Str)]);
    assert_eq!(next, LineState::Continued);
    let (tokens, next) = props("    c", next);
    assert_eq!(tokens, [("c", Str)]);
    assert_eq!(next, LineState::Clear);
    // Two backslashes are one escaped backslash: nothing goes on.
    assert_eq!(
        props("path=c:\\\\dir\\\\", LineState::Clear).1,
        LineState::Clear
    );
}
