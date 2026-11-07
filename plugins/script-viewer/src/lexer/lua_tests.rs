use super::continues;
use crate::lang::Lang;
use crate::test_support::{class_of, lexed, texts_of};
use crate::types::TokenClass::{
    Comment, Function, Key, Keyword, Literal, Number, Operator, Punctuation, String,
};

/// A Lua file with what sets the language apart: `#!`, line and long comments, a long string over
/// lines, escapes, `~=`, method calls, hexadecimal numbers and exponents, `#`, `...`, a label, the
/// name of a function in parts, and a short string its line leaves open.
const SAMPLE: &str = r#"#!/usr/bin/env lua
-- a comment
local M = {}
--[==[ a long
comment ]==]
function M.greet(name, ...)
  local s = [[multi
line]] .. "x\"y" .. 'z'
  if name ~= nil then print(("hi %s"):format(name)) end
  return 0x1F + 1.5e3 + #name
end
::continue::
local t = "open
local u = 1"#;

#[test]
fn a_lua_sample_is_coloured_like_an_editor() {
    let lines = lexed(Lang::Lua, SAMPLE);
    assert_eq!(lines[0], vec![("#!/usr/bin/env lua", Comment)]);
    assert_eq!(lines[1], vec![("-- a comment", Comment)]);
    assert_eq!(
        lines[2],
        vec![
            ("local", Keyword),
            ("=", Operator),
            ("{", Punctuation),
            ("}", Punctuation)
        ]
    );
    assert_eq!(lines[3], vec![("--[==[ a long", Comment)]);
    assert_eq!(lines[4], vec![("comment ]==]", Comment)]);
    assert_eq!(
        lines[5],
        vec![
            ("function", Keyword),
            ("M", Function),
            (".", Punctuation),
            ("greet", Function),
            ("(", Punctuation),
            (",", Punctuation),
            ("...", Operator),
            (")", Punctuation)
        ]
    );
    assert_eq!(texts_of(&lines[6..7], String), vec!["[[multi"]);
    assert_eq!(
        lines[7],
        vec![
            ("line]]", String),
            ("..", Operator),
            ("\"x\\\"y\"", String),
            ("..", Operator),
            ("'z'", String)
        ]
    );
    assert_eq!(class_of(&lines[8..9], "~="), Some(Operator));
    assert_eq!(class_of(&lines[8..9], "nil"), Some(Literal));
    assert_eq!(class_of(&lines[8..9], "print"), Some(Function));
    assert_eq!(class_of(&lines[8..9], "format"), Some(Function));
    assert_eq!(
        lines[9],
        vec![
            ("return", Keyword),
            ("0x1F", Number),
            ("+", Operator),
            ("1.5e3", Number),
            ("+", Operator),
            ("#", Operator)
        ]
    );
    assert_eq!(lines[11], vec![("::continue::", Key)]);
    // The open string ends with its line; the next line is code.
    assert_eq!(texts_of(&lines[12..13], String), vec!["\"open"]);
    assert_eq!(texts_of(&lines[13..], Number), vec!["1"]);
}

#[test]
fn a_backslash_or_z_carries_a_short_string_on() {
    let lines = lexed(Lang::Lua, "s = \"a\\\nb\" .. 'c\\z  \n  d'\nx = 1");
    assert_eq!(texts_of(&lines[1..2], String), vec!["b\"", "'c\\z  "]);
    assert_eq!(lines[2], vec![("  d'", String)]);
    assert_eq!(texts_of(&lines[3..], Number), vec!["1"]);
    assert!(continues(b"\"a\\"));
    assert!(!continues(b"\"a\\\\"));
    assert!(continues(b"'c\\z  "));
    assert!(!continues(b"'c\\\\z"));
    assert!(!continues(b"'open"));
}

#[test]
fn long_brackets_close_only_at_their_own_level() {
    let lines = lexed(Lang::Lua, "x = [==[ a ]] b ]=] c\n]==] y = 2");
    assert_eq!(
        lines[0],
        vec![("=", Operator), ("[==[ a ]] b ]=] c", String)]
    );
    assert_eq!(
        lines[1],
        vec![("]==]", String), ("=", Operator), ("2", Number)]
    );
    let lines = lexed(Lang::Lua, "t[i] = 1 -- [[ not long");
    assert_eq!(class_of(&lines, "-- [[ not long"), Some(Comment));
    assert_eq!(class_of(&lines, "["), Some(Punctuation));
}
