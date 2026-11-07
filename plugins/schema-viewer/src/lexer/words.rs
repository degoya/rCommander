//! The words of both languages - keywords, constants and built-in types, each table sorted - what
//! a keyword declares, and the byte classes. Case matters in both languages.

use super::Mode;
use crate::types::TokenClass;

/// A sorted table of words.
type Table = &'static [&'static [u8]];

/// Protobuf's keywords, proto2, proto3 and editions alike.
const PROTO_KEYWORDS: Table = &[
    b"edition",
    b"enum",
    b"extend",
    b"extensions",
    b"group",
    b"import",
    b"map",
    b"max",
    b"message",
    b"oneof",
    b"option",
    b"optional",
    b"package",
    b"public",
    b"repeated",
    b"required",
    b"reserved",
    b"returns",
    b"rpc",
    b"service",
    b"stream",
    b"syntax",
    b"to",
    b"weak",
];
/// Protobuf's constants: the booleans and the floats' `inf` and `nan`.
const PROTO_LITERALS: Table = &[b"false", b"inf", b"nan", b"true"];
/// Protobuf's scalar types.
const PROTO_TYPES: Table = &[
    b"bool",
    b"bytes",
    b"double",
    b"fixed32",
    b"fixed64",
    b"float",
    b"int32",
    b"int64",
    b"sfixed32",
    b"sfixed64",
    b"sint32",
    b"sint64",
    b"string",
    b"uint32",
    b"uint64",
];

/// GraphQL's keywords: operations, fragments, the type system and its extensions.
const GRAPHQL_KEYWORDS: Table = &[
    b"directive",
    b"enum",
    b"extend",
    b"fragment",
    b"implements",
    b"input",
    b"interface",
    b"mutation",
    b"on",
    b"query",
    b"repeatable",
    b"scalar",
    b"schema",
    b"subscription",
    b"type",
    b"union",
];
/// GraphQL's constants.
const GRAPHQL_LITERALS: Table = &[b"false", b"null", b"true"];
/// GraphQL's built-in scalars.
const GRAPHQL_TYPES: Table = &[b"Boolean", b"Float", b"ID", b"Int", b"String"];

/// The class of `word` in the tables `keywords`, `literals` and `types`, or none.
fn classify(word: &[u8], keywords: Table, literals: Table, types: Table) -> Option<TokenClass> {
    let has = |table: Table| table.binary_search(&word).is_ok();
    if has(keywords) {
        Some(TokenClass::Keyword)
    } else if has(literals) {
        Some(TokenClass::Literal)
    } else if has(types) {
        Some(TokenClass::TypeName)
    } else {
        None
    }
}

/// The class of the Protobuf word `word` alone: a keyword, a constant, a scalar type, or none.
pub(crate) fn proto_class(word: &[u8]) -> Option<TokenClass> {
    classify(word, PROTO_KEYWORDS, PROTO_LITERALS, PROTO_TYPES)
}

/// The class of the GraphQL word `word` alone: a keyword, a constant, a built-in scalar, or
/// none.
pub(crate) fn graphql_class(word: &[u8]) -> Option<TokenClass> {
    classify(word, GRAPHQL_KEYWORDS, GRAPHQL_LITERALS, GRAPHQL_TYPES)
}

/// What the Protobuf keyword `word` makes of the next name.
pub(crate) fn proto_declares(word: &[u8]) -> Mode {
    match word {
        b"message" | b"enum" | b"service" | b"extend" | b"group" => Mode::TypeName,
        b"rpc" => Mode::FunctionName,
        _ => Mode::Expr,
    }
}

/// What the GraphQL keyword `word` makes of the next name.
pub(crate) fn graphql_declares(word: &[u8]) -> Mode {
    match word {
        b"type" | b"interface" | b"union" | b"enum" | b"input" | b"scalar" | b"implements"
        | b"on" => Mode::TypeName,
        b"query" | b"mutation" | b"subscription" | b"fragment" => Mode::FunctionName,
        _ => Mode::Expr,
    }
}

/// A blank between tokens.
pub(crate) fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | 0x0B | 0x0C)
}

pub(crate) fn is_digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

/// A byte a name starts with: a letter or `_` - both languages' names are ASCII.
pub(crate) fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// A byte of a name.
pub(crate) fn is_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;
