use crate::lexer::LineState;
use crate::test_support::pieces;
use crate::types::Format;
use crate::types::TokenClass::{
    self, Comment, Invalid, Key, Keyword, Number, Operator, Punctuation, String as Str, TypeName,
};

fn reg(line: &str, state: LineState) -> (Vec<(&str, TokenClass)>, LineState) {
    pieces(line, state, Format::Registry)
}

fn clear(line: &str) -> Vec<(&str, TokenClass)> {
    let (tokens, next) = reg(line, LineState::Clear);
    assert_eq!(next, LineState::Clear, "{line}");
    tokens
}

#[test]
fn the_version_keys_and_comments() {
    assert_eq!(
        clear("Windows Registry Editor Version 5.00"),
        [("Windows Registry Editor Version 5.00", Keyword)]
    );
    assert_eq!(clear("REGEDIT4"), [("REGEDIT4", Keyword)]);
    assert_eq!(
        clear("[HKEY_CURRENT_USER\\Software\\Test]"),
        [("[HKEY_CURRENT_USER\\Software\\Test]", Keyword)]
    );
    assert_eq!(
        clear("[-HKEY_CURRENT_USER\\Old]"),
        [("[-HKEY_CURRENT_USER\\Old]", Keyword)]
    );
    assert_eq!(clear("; exported"), [("; exported", Comment)]);
    assert_eq!(clear("garbage"), [("garbage", Invalid)]);
}

#[test]
fn values_of_every_type() {
    assert_eq!(
        clear("\"Name\"=\"Value \\\"q\\\"\""),
        [
            ("\"Name\"", Key),
            ("=", Punctuation),
            ("\"Value \\\"q\\\"\"", Str)
        ]
    );
    assert_eq!(
        clear("@=\"default\""),
        [("@", Key), ("=", Punctuation), ("\"default\"", Str)]
    );
    assert_eq!(
        clear("\"Count\"=dword:0000001f"),
        [
            ("\"Count\"", Key),
            ("=", Punctuation),
            ("dword:", TypeName),
            ("0000001f", Number)
        ]
    );
    assert_eq!(
        clear("\"Bad\"=dword:1f"),
        [
            ("\"Bad\"", Key),
            ("=", Punctuation),
            ("dword:", TypeName),
            ("1f", Invalid)
        ]
    );
    assert_eq!(
        clear("\"Gone\"=-"),
        [("\"Gone\"", Key), ("=", Punctuation), ("-", Operator)]
    );
    assert_eq!(
        clear("\"x\"=qword:1"),
        [("\"x\"", Key), ("=", Punctuation), ("qword:1", Invalid)]
    );
    assert_eq!(
        clear("\"Bin\"=hex:0a,ff"),
        [
            ("\"Bin\"", Key),
            ("=", Punctuation),
            ("hex:", TypeName),
            ("0a", Number),
            (",", Punctuation),
            ("ff", Number)
        ]
    );
}

#[test]
fn a_hex_list_goes_on_after_a_backslash() {
    let (tokens, next) = reg("\"Path\"=hex(2):25,00,\\", LineState::Clear);
    assert_eq!(
        tokens,
        [
            ("\"Path\"", Key),
            ("=", Punctuation),
            ("hex(2):", TypeName),
            ("25", Number),
            (",", Punctuation),
            ("00", Number),
            (",", Punctuation),
            ("\\", Punctuation)
        ]
    );
    assert_eq!(next, LineState::Continued);
    let (tokens, next) = reg("  79,00,\\", LineState::Continued);
    assert_eq!(next, LineState::Continued);
    assert_eq!(tokens.len(), 5);
    let (tokens, next) = reg("  00,0", LineState::Continued);
    assert_eq!(next, LineState::Clear);
    assert_eq!(tokens, [("00", Number), (",", Punctuation), ("0", Invalid)]);
}
