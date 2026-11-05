//! The shell form of `RUN`, `CMD` and `ENTRYPOINT`, and the body of a `RUN <<EOF` heredoc, in
//! the shell viewer's colours as far as one line shows them: a keyword (`if`, `for`, `do`) a
//! keyword, a builtin (`cd`, `export`, `set`) a function, operators (`&&`, `|`, `;`, `>`)
//! operators, variables keys, strings strings, a comment from a `#` that starts a word, a number
//! as an argument. Without a second grammar: what the shell viewer's lexer keeps over lines
//! (substitutions, `case` patterns) is not followed - only a quote left open at a continued
//! line's end, and whether the next word starts a command.

use super::LineState;
use super::cursor::{Cursor, is_blank, is_name_byte};
use crate::types::TokenClass;

/// Reserved words at a command's start, and whether the word after one starts a command again.
const KEYWORDS: [(&[u8], bool); 19] = [
    (b"if", true),
    (b"then", true),
    (b"elif", true),
    (b"else", true),
    (b"do", true),
    (b"while", true),
    (b"until", true),
    (b"time", true),
    (b"fi", false),
    (b"done", false),
    (b"esac", false),
    (b"in", false),
    (b"[[", false),
    (b"]]", false),
    (b"case", false),
    (b"for", false),
    (b"select", false),
    (b"function", false),
    (b"!", true),
];

/// The builtins of bash and POSIX sh, coloured as functions (the shell viewer's list).
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

/// Lexes the shell text from the cursor to the end of the line.
pub(super) fn lex(cursor: &mut Cursor<'_>, state: &mut LineState) {
    if let Some(quote) = state.quote {
        if !cursor.quoted(quote, false, quote == b'"') {
            return;
        }
        state.quote = None;
        state.command_start = false;
    }
    loop {
        cursor.blanks();
        let Some(byte) = cursor.peek() else {
            return;
        };
        match byte {
            b'#' => {
                let start = cursor.at;
                cursor.rest();
                cursor.emit(start, TokenClass::Comment);
                return;
            }
            b'"' | b'\'' => {
                state.command_start = false;
                if !cursor.quoted(byte, true, byte == b'"') {
                    state.quote = Some(byte);
                    return;
                }
            }
            b'$' if cursor.peek_at(1) == Some(b'(') => {
                let arithmetic = cursor.peek_at(2) == Some(b'(');
                cursor.token(2 + usize::from(arithmetic), TokenClass::Punctuation);
                state.command_start = !arithmetic;
            }
            b'$' => {
                if !cursor.variable() {
                    cursor.bump(1);
                }
                state.command_start = false;
            }
            b'`' if cursor.escape == b'\\' => {
                cursor.token(1, TokenClass::Punctuation);
                state.command_start = true;
            }
            b'&' | b'|' | b';' => {
                let doubled = cursor.peek_at(1) == Some(byte);
                cursor.token(1 + usize::from(doubled), TokenClass::Operator);
                state.command_start = true;
            }
            b'(' | b')' => {
                cursor.token(1, TokenClass::Punctuation);
                state.command_start = byte == b'(';
            }
            b'<' if cursor.starts_with(b"<<") && !cursor.starts_with(b"<<<") => {
                match cursor.heredoc(state.command_start) {
                    Some(marker) => state.add_heredoc(marker),
                    None => cursor.token(2, TokenClass::Operator),
                }
            }
            b'<' | b'>' => {
                let start = cursor.at;
                cursor.take_while(|byte| matches!(byte, b'<' | b'>' | b'&'));
                cursor.emit(start, TokenClass::Operator);
            }
            _ => word(cursor, state),
        }
    }
}

/// Whether `byte` ends a plain word.
fn is_meta(byte: u8) -> bool {
    is_blank(byte) || b";&|()<>\"'`$".contains(&byte)
}

/// A plain word: at a command's start a keyword, a builtin, an assignment or a command of no
/// colour; else a number or an argument of no colour.
fn word(cursor: &mut Cursor<'_>, state: &mut LineState) {
    let start = cursor.at;
    while let Some(byte) = cursor.peek() {
        if byte == cursor.escape {
            cursor.escaped();
        } else if is_meta(byte) {
            break;
        } else {
            cursor.bump(1);
        }
    }
    if cursor.at == start {
        // A byte no rule takes (a lone escape at the end): on past it.
        cursor.bump(1);
        return;
    }
    let text = cursor.since(start);
    if text.iter().all(u8::is_ascii_digit) && matches!(cursor.peek(), Some(b'<' | b'>')) {
        // `2>&1`: the descriptor belongs to the redirection.
        cursor.emit(start, TokenClass::Operator);
        return;
    }
    if !state.command_start {
        if is_number(text) {
            cursor.emit(start, TokenClass::Number);
        }
        return;
    }
    if let Some(name) = assignment(text) {
        // `VAR=value cmd`: the command is still to come.
        cursor.span(start, start + name, TokenClass::Key);
        let operator = if text.get(name) == Some(&b'+') { 2 } else { 1 };
        cursor.span(start + name, start + name + operator, TokenClass::Operator);
        return;
    }
    if let Some((_, again)) = KEYWORDS.iter().find(|(keyword, _)| *keyword == text) {
        cursor.emit(
            start,
            if text == b"!" {
                TokenClass::Operator
            } else {
                TokenClass::Keyword
            },
        );
        state.command_start = *again;
        return;
    }
    if matches!(text, b"{" | b"}") {
        cursor.emit(start, TokenClass::Punctuation);
        return;
    }
    if BUILTINS.contains(&text) {
        cursor.emit(start, TokenClass::Function);
    }
    state.command_start = false;
}

/// The length of the name of `NAME=` or `NAME+=` at the start of `text`.
fn assignment(text: &[u8]) -> Option<usize> {
    let name = text.iter().take_while(|byte| is_name_byte(**byte)).count();
    let starts = text.first().is_some_and(|byte| !byte.is_ascii_digit());
    let operator = text.get(name..)?;
    (name > 0 && starts && (operator.starts_with(b"=") || operator.starts_with(b"+=")))
        .then_some(name)
}

/// Digits, with a fraction perhaps (`sleep 0.5`).
fn is_number(text: &[u8]) -> bool {
    let (whole, fraction) = match text.iter().position(|byte| *byte == b'.') {
        Some(dot) => (&text[..dot], Some(&text[dot + 1..])),
        None => (text, None),
    };
    let digits = |part: &[u8]| !part.is_empty() && part.iter().all(u8::is_ascii_digit);
    digits(whole) && fraction.is_none_or(digits)
}
