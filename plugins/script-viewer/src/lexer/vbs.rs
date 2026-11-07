//! VBScript, a line at a time - nothing in it reaches past a line: `'` and `Rem` comments,
//! strings with `""` for a quote, dates `#1/31/2026#`, numbers with `&H` and `&O`, keywords in any
//! case, the name after `Sub`, `Function`, `Property Get|Let|Set` and `Class` (but not after
//! `End` or `Exit`), and ` _` continuing a statement.

use super::scan::is_name_start;
use super::{Cursor, Emit};
use crate::lang::Lang;
use crate::types::TokenClass;

/// The most bytes between a date's `#` signs.
const MAX_DATE: usize = 32;

/// What the word before declares: the next name is coloured as what it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Declares {
    /// `Sub`, `Function`, `Property Get`: a procedure.
    Procedure,
    /// `Class`: a type.
    Type,
}

/// Lexes one line of VBScript.
pub(crate) fn line<E: Emit>(line: &[u8], emit: &mut E) {
    let mut cur = Cursor::new(line, emit);
    let mut declares = None;
    // `End Sub`, `Exit Function`: the keyword after them closes or leaves, it declares nothing.
    let mut closing = false;
    while let Some(byte) = cur.byte(cur.pos) {
        let start = cur.pos;
        if byte == b' ' || byte == b'\t' {
            cur.pos += 1;
            continue;
        }
        if is_name_start(byte) {
            closing = word(&mut cur, &mut declares, closing);
            continue;
        }
        declares = None;
        closing = false;
        match byte {
            b'\'' => cur.rest(TokenClass::Comment),
            b'"' => string(&mut cur),
            b'#' => match (start + 1..(start + 2 + MAX_DATE).min(cur.end))
                .find(|&at| cur.byte(at) == Some(b'#'))
            {
                Some(close) if close > start + 1 && date(&cur.line[start + 1..close]) => {
                    cur.push(start, close + 1, TokenClass::Number);
                    cur.pos = close + 1;
                }
                _ => cur.symbol(),
            },
            b'&' if matches!(cur.byte(start + 1), Some(b'h' | b'H' | b'o' | b'O'))
                && cur.byte(start + 2).is_some_and(|b| b.is_ascii_hexdigit()) =>
            {
                let mut end = start + 2;
                while cur
                    .byte(end)
                    .is_some_and(|b| b.is_ascii_hexdigit() || b == b'&')
                {
                    end += 1;
                }
                cur.push(start, end, TokenClass::Number);
                cur.pos = end;
            }
            b'0'..=b'9' => cur.number(),
            b'.' if cur.byte(start + 1).is_some_and(|b| b.is_ascii_digit()) => cur.number(),
            _ => cur.symbol(),
        }
    }
}

/// A string up to its closing quote, `""` a quote inside; a line that ends it unclosed is an
/// error.
fn string<E: Emit>(cur: &mut Cursor<'_, E>) {
    let start = cur.pos;
    cur.opened(start, 1);
    let mut at = start + 1;
    while let Some(byte) = cur.byte(at) {
        if byte == b'"' {
            if cur.byte(at + 1) == Some(b'"') {
                at += 2;
                continue;
            }
            cur.push(start, at + 1, TokenClass::String);
            cur.pos = at + 1;
            cur.closed();
            return;
        }
        at += 1;
    }
    cur.rest(TokenClass::String);
    cur.failed();
}

/// Whether `inside` is a date's or time's text: digits, separators, blanks and AM/PM.
fn date(inside: &[u8]) -> bool {
    inside.iter().any(u8::is_ascii_digit)
        && inside
            .iter()
            .all(|byte| byte.is_ascii_digit() || b"/-:., aApPmM".contains(byte))
}

/// A name: a keyword, a constant, a built-in function, a declared name, `Rem` and its comment, or
/// the continuation `_`. Returns whether it is `End` or `Exit`.
fn word<E: Emit>(cur: &mut Cursor<'_, E>, declares: &mut Option<Declares>, closing: bool) -> bool {
    let start = cur.pos;
    let end = cur.name_end(start);
    let lower = cur.line[start..end].to_ascii_lowercase();
    if lower == b"rem"
        && cur
            .byte(end)
            .is_none_or(|byte| byte == b' ' || byte == b'\t')
    {
        cur.rest(TokenClass::Comment);
        return false;
    }
    let class = if lower == b"_" {
        Some(TokenClass::Operator)
    } else {
        match *declares {
            // `Property Get Name`: the accessor's keyword comes first.
            Some(Declares::Procedure) if matches!(lower.as_slice(), b"get" | b"let" | b"set") => {
                Some(TokenClass::Keyword)
            }
            Some(Declares::Procedure) => Some(TokenClass::Function),
            Some(Declares::Type) => Some(TokenClass::TypeName),
            None => Lang::VbScript.classify(&lower),
        }
    };
    if declares.is_some() && class != Some(TokenClass::Keyword) {
        *declares = None;
    } else if class == Some(TokenClass::Keyword) && !closing {
        match lower.as_slice() {
            b"sub" | b"function" | b"property" => *declares = Some(Declares::Procedure),
            b"class" => *declares = Some(Declares::Type),
            _ => {}
        }
    }
    if let Some(class) = class {
        cur.push(start, end, class);
    }
    cur.pos = end;
    class == Some(TokenClass::Keyword) && matches!(lower.as_slice(), b"end" | b"exit")
}

#[cfg(test)]
#[path = "vbs_tests.rs"]
mod tests;
