//! The shell's words, without a file: which bytes end a word, the keywords and builtins, and the
//! shapes the lexer looks ahead for - an assignment, a function's `()`, a redirection's fd.

use super::state::Pos;

/// The longest word told apart by its text: no keyword, builtin or number is longer.
pub(crate) const WORD_BYTES: usize = 64;

/// Whether `byte` ends a word: a blank, a line break or an operator character.
pub(crate) fn is_meta(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\t' | b'\r' | b'\n' | b';' | b'&' | b'|' | b'(' | b')' | b'<' | b'>'
    )
}

/// Whether `byte` ends a plain piece of a word: a meta character, or a quote, `$`, backtick or
/// `\`, which go on with the word in a piece of their own.
pub(crate) fn ends_plain(byte: u8) -> bool {
    is_meta(byte) || matches!(byte, b'"' | b'\'' | b'`' | b'$' | b'\\')
}

/// Whether `byte` may start a variable's name.
pub(crate) fn name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

/// Whether `byte` may go on with a variable's name.
pub(crate) fn name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Whether `$` followed by `byte` expands: `$name`, `$1`, `$@` and the other special
/// parameters, `${`, `$(`.
pub(crate) fn expands(byte: u8) -> bool {
    name_start(byte) || byte.is_ascii_digit() || b"{(@*#?$!-".contains(&byte)
}

/// Whether `text` is a variable's name.
pub(crate) fn is_name(text: &[u8]) -> bool {
    text.first().is_some_and(|byte| name_start(*byte)) && text.iter().all(|byte| name_byte(*byte))
}

/// A reserved word at a command's start, and where the word after it stands.
pub(crate) fn keyword(text: &[u8]) -> Option<Pos> {
    Some(match text {
        b"if" | b"then" | b"elif" | b"else" | b"do" | b"while" | b"until" | b"time" => Pos::Command,
        b"fi" | b"done" | b"esac" | b"in" | b"[[" | b"]]" => Pos::Args,
        b"case" => Pos::CaseWord,
        b"for" | b"select" => Pos::ForVar,
        b"function" => Pos::FuncName,
        _ => return None,
    })
}

/// The builtins of bash and POSIX sh, coloured as functions.
const BUILTINS: [&[u8]; 60] = [
    b".",
    b":",
    b"[",
    b"alias",
    b"bg",
    b"bind",
    b"break",
    b"builtin",
    b"caller",
    b"cd",
    b"command",
    b"compgen",
    b"complete",
    b"compopt",
    b"continue",
    b"declare",
    b"dirs",
    b"disown",
    b"echo",
    b"enable",
    b"eval",
    b"exec",
    b"exit",
    b"export",
    b"false",
    b"fc",
    b"fg",
    b"getopts",
    b"hash",
    b"help",
    b"history",
    b"jobs",
    b"kill",
    b"let",
    b"local",
    b"logout",
    b"mapfile",
    b"popd",
    b"printf",
    b"pushd",
    b"pwd",
    b"read",
    b"readarray",
    b"readonly",
    b"return",
    b"set",
    b"shift",
    b"shopt",
    b"source",
    b"suspend",
    b"test",
    b"times",
    b"trap",
    b"true",
    b"type",
    b"typeset",
    b"ulimit",
    b"umask",
    b"unalias",
    b"unset",
];

/// Whether `text` is a builtin.
pub(crate) fn is_builtin(text: &[u8]) -> bool {
    BUILTINS.contains(&text)
}

/// Whether the builtin `text` declares variables: its arguments may be assignments.
pub(crate) fn declares(text: &[u8]) -> bool {
    matches!(
        text,
        b"declare" | b"export" | b"local" | b"readonly" | b"typeset"
    )
}

/// Whether `text` is a number: digits, with a fraction perhaps (`sleep 0.5`).
pub(crate) fn is_number(text: &[u8]) -> bool {
    let (whole, fraction) = match text.iter().position(|byte| *byte == b'.') {
        Some(at) => (&text[..at], Some(&text[at + 1..])),
        None => (text, None),
    };
    let digits = |part: &[u8]| !part.is_empty() && part.iter().all(u8::is_ascii_digit);
    digits(whole) && fraction.is_none_or(digits)
}

/// An assignment at the start of `bytes`: the length of its name (with an index `[...]`) and of
/// its operator (`=` 1, `+=` 2).
pub(crate) fn assignment(bytes: &[u8]) -> Option<(usize, usize)> {
    if !name_start(*bytes.first()?) {
        return None;
    }
    let mut at = bytes
        .iter()
        .position(|byte| !name_byte(*byte))
        .unwrap_or(bytes.len());
    if bytes.get(at) == Some(&b'[') {
        let index = bytes.get(at..)?;
        let close = index.iter().position(|byte| *byte == b']')?;
        if index[..close].iter().any(|byte| is_meta(*byte)) {
            return None;
        }
        at += close + 1;
    }
    match (bytes.get(at), bytes.get(at + 1)) {
        (Some(b'='), _) => Some((at, 1)),
        (Some(b'+'), Some(b'=')) => Some((at, 2)),
        _ => None,
    }
}

/// Whether `bytes`, after a word, are a function definition's `()`, blanks allowed around `(`.
pub(crate) fn function_parens(bytes: &[u8]) -> bool {
    let mut rest = bytes
        .iter()
        .copied()
        .filter(|byte| !matches!(byte, b' ' | b'\t'));
    rest.next() == Some(b'(') && rest.next() == Some(b')')
}

/// The length of the fd number at the start of `bytes` when a redirection follows it (`2>`,
/// `2>&1`, `0<`).
pub(crate) fn fd_prefix(bytes: &[u8]) -> Option<usize> {
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    (digits > 0 && matches!(bytes.get(digits), Some(b'<' | b'>'))).then_some(digits)
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;
