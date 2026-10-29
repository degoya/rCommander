//! The pieces of a line the lexer recognises by their bytes alone: names, variables, numbers,
//! redirections, keywords and cmdlet names. Every decision is on ASCII; a byte of 0x80 and above
//! is part of a name, so no piece starts or ends inside a character.

use crate::types::TokenClass;

/// The keywords, matched in any case where a statement starts.
const KEYWORDS: &[&[u8]] = &[
    b"if",
    b"elseif",
    b"else",
    b"switch",
    b"foreach",
    b"for",
    b"while",
    b"do",
    b"until",
    b"function",
    b"filter",
    b"param",
    b"begin",
    b"process",
    b"end",
    b"return",
    b"break",
    b"continue",
    b"try",
    b"catch",
    b"finally",
    b"throw",
    b"trap",
    b"class",
    b"enum",
    b"using",
    b"exit",
];

/// The variables coloured as `literal`.
const LITERALS: &[&[u8]] = &[b"true", b"false", b"null"];

/// The type suffixes of a number (`1l`, `2ul`, `3d`, `4n`), the longer before the shorter.
const SUFFIXES: &[&[u8]] = &[b"ul", b"uy", b"us", b"l", b"d", b"u", b"y", b"s", b"n"];

/// The multipliers of a number: `10kb`.
const MULTIPLIERS: &[&[u8]] = &[b"kb", b"mb", b"gb", b"tb", b"pb"];

/// A byte of a name: a letter, a digit, `_`, or part of a character beyond ASCII.
pub(crate) fn is_name(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

/// A byte a name starts with.
pub(crate) fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte >= 0x80
}

/// Blank: a space, a tab, a form feed, a stray `\r`.
pub(crate) fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0B | 0x0C | b'\r')
}

/// A byte that ends a bare word: a blank or a character of the syntax. `=` ends one only where
/// an expression is read; among a command's arguments it is text (`a=b`).
pub(crate) fn ends_word(byte: u8, args: bool) -> bool {
    is_blank(byte) || b"(){}[];,|&<>\"'$`".contains(&byte) || (!args && byte == b'=')
}

/// The length of the UTF-8 character `lead` starts; 1 for anything else.
pub(crate) fn char_width(lead: u8) -> usize {
    match lead {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 1,
    }
}

/// The end of the name run from `at`.
pub(crate) fn name_end(line: &[u8], at: usize) -> usize {
    at + line
        .get(at..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| is_name(**byte))
        .count()
}

/// The end of the bare word from `at`; at least one character.
pub(crate) fn word_end(line: &[u8], at: usize, args: bool) -> usize {
    let first = line.get(at).map_or(1, |byte| char_width(*byte));
    let rest = line.get(at + first..).unwrap_or_default();
    (at + first
        + rest
            .iter()
            .take_while(|byte| !ends_word(**byte, args))
            .count())
    .min(line.len())
}

/// The end of the variable whose `$` stands at `at`: `$name`, `$scope:name`, `$env:PATH`,
/// `${any name}`, `$_`, `$$`, `$?`, `$^`. `None` for a `$` that starts none (`$(` is not one).
pub(crate) fn variable_end(line: &[u8], at: usize) -> Option<usize> {
    let next = *line.get(at + 1)?;
    match next {
        b'{' => {
            let close = line.get(at + 2..)?.iter().position(|byte| *byte == b'}')?;
            Some(at + 2 + close + 1)
        }
        b'$' | b'?' | b'^' => Some(at + 2),
        _ if is_name(next) => {
            let mut end = name_end(line, at + 1);
            // A scope or a drive: `$script:x`, `$env:PATH`; never `$x::y`.
            if line.get(end) == Some(&b':') && line.get(end + 1).is_some_and(|byte| is_name(*byte))
            {
                end = name_end(line, end + 1);
            }
            Some(end)
        }
        _ => None,
    }
}

/// The class of the variable `$...` in `variable`: `literal` for `$true`, `$false`, `$null`.
pub(crate) fn variable_class(variable: &[u8]) -> TokenClass {
    let name = variable.get(1..).unwrap_or_default();
    if LITERALS
        .iter()
        .any(|literal| name.eq_ignore_ascii_case(literal))
    {
        TokenClass::Literal
    } else {
        TokenClass::Key
    }
}

/// The end of the number at `at`: `42`, `1.5e3`, `.5`, `0x1F`, a type suffix (`10l`) and a
/// multiplier (`10kb`, `1.5GB`). `None` when a name goes on right after it (`7zip`).
pub(crate) fn number_end(line: &[u8], at: usize) -> Option<usize> {
    let byte = |index: usize| line.get(index).copied();
    let digits = |from: usize, hex: bool| {
        from + line
            .get(from..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| {
                if hex {
                    byte.is_ascii_hexdigit()
                } else {
                    byte.is_ascii_digit()
                }
            })
            .count()
    };
    let hex = byte(at) == Some(b'0')
        && matches!(byte(at + 1), Some(b'x' | b'X'))
        && byte(at + 2).is_some_and(|byte| byte.is_ascii_hexdigit());
    let mut end = if hex {
        digits(at + 2, true)
    } else {
        digits(at, false)
    };
    if !hex {
        if byte(end) == Some(b'.') && byte(end + 1).is_some_and(|byte| byte.is_ascii_digit()) {
            end = digits(end + 1, false);
        }
        if end == at {
            return None;
        }
        if matches!(byte(end), Some(b'e' | b'E')) {
            let sign = usize::from(matches!(byte(end + 1), Some(b'+' | b'-')));
            if byte(end + 1 + sign).is_some_and(|byte| byte.is_ascii_digit()) {
                end = digits(end + 1 + sign, false);
            }
        }
    }
    let starts = |end: usize, word: &[u8]| {
        line.get(end..end + word.len())
            .is_some_and(|got| got.eq_ignore_ascii_case(word))
    };
    if let Some(suffix) = SUFFIXES.iter().find(|suffix| starts(end, suffix)) {
        end += suffix.len();
    }
    if let Some(multiplier) = MULTIPLIERS
        .iter()
        .find(|multiplier| starts(end, multiplier))
    {
        end += multiplier.len();
    }
    match byte(end) {
        Some(next) if is_name(next) => None,
        _ => Some(end),
    }
}

/// Whether the whole of `word` is a number.
pub(crate) fn is_number(word: &[u8]) -> bool {
    number_end(word, 0) == Some(word.len())
}

/// The end of the redirection at `at`: `>`, `>>`, `<`, `2>`, `2>>`, `2>&1`, `*>`, `*>&1`.
pub(crate) fn redirect_end(line: &[u8], at: usize) -> Option<usize> {
    let byte = |index: usize| line.get(index).copied();
    let mut end = at;
    match byte(end)? {
        b'<' => return Some(at + 1),
        b'1'..=b'6' | b'*' if byte(end + 1) == Some(b'>') => end += 2,
        b'>' => end += 1,
        _ => return None,
    }
    match byte(end) {
        Some(b'>') => end += 1,
        Some(b'&') if matches!(byte(end + 1), Some(b'1' | b'2')) => end += 2,
        _ => {}
    }
    Some(end)
}

/// A keyword, in any case.
pub(crate) fn is_keyword(word: &[u8]) -> bool {
    KEYWORDS
        .iter()
        .any(|keyword| word.eq_ignore_ascii_case(keyword))
}

/// A cmdlet's or advanced function's name: `Verb-Noun`, letters before the `-`, a letter and
/// then letters, digits or `_` after it (`Get-ChildItem`, `ConvertTo-Json`, `Get-AzVM`).
pub(crate) fn is_verb_noun(word: &[u8]) -> bool {
    let Some(dash) = word.iter().position(|byte| *byte == b'-') else {
        return false;
    };
    let (verb, noun) = (&word[..dash], &word[dash + 1..]);
    !verb.is_empty()
        && verb.iter().all(u8::is_ascii_alphabetic)
        && noun.first().is_some_and(u8::is_ascii_alphabetic)
        && noun
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

/// A byte of a type's name inside `[...]`: a name, `.`, `` ` `` (a generic's arity), `+` (a
/// nested type).
pub(crate) fn is_type_name(byte: u8) -> bool {
    is_name(byte) || matches!(byte, b'.' | b'`' | b'+')
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;
