//! The pieces of a line the lexer recognises by their bytes alone: names, numbers, operators,
//! string prefixes, keywords, soft keywords and built-in names. Every decision is on ASCII; a byte
//! of 0x80 and above is part of a name, so no piece starts or ends inside a character.

use crate::types::TokenClass;

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

/// The end of the name run from `at`.
pub(crate) fn name_end(line: &[u8], at: usize) -> usize {
    at + line
        .get(at..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| is_name(**byte))
        .count()
}

/// The first byte from `at` that is not blank.
fn skip_blanks(line: &[u8], at: usize) -> usize {
    at + line
        .get(at..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| is_blank(**byte))
        .count()
}

/// Whether `word`, standing right before a quote, is a string prefix: `r`, `u`, `b`, `f`, `t`,
/// `br`, `rb`, `fr`, `rf`, `tr`, `rt`, in any case.
pub(crate) fn is_string_prefix(word: &[u8]) -> bool {
    let lower = |at: usize| word.get(at).map(u8::to_ascii_lowercase);
    match (word.len(), lower(0), lower(1)) {
        (1, Some(b'r' | b'u' | b'b' | b'f' | b't'), _) => true,
        (2, Some(first), Some(second)) => matches!(
            (first, second),
            (b'b' | b'f' | b't', b'r') | (b'r', b'b' | b'f' | b't')
        ),
        _ => false,
    }
}

/// The end of the number at `at`, a digit or a `.` before one: `42`, `1_000`, `0x_FF`, `0o17`,
/// `0b1010`, `1.5`, `.5`, `1.`, `6.02e+23`, and the imaginary `2j`, `1.5e3J`.
pub(crate) fn number_end(line: &[u8], at: usize) -> usize {
    let run = |from: usize, digit: fn(&u8) -> bool| {
        from + line
            .get(from..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| digit(byte) || **byte == b'_')
            .count()
    };
    if line.get(at) == Some(&b'0') {
        let radix: Option<fn(&u8) -> bool> = match line.get(at + 1) {
            Some(b'x' | b'X') => Some(u8::is_ascii_hexdigit),
            Some(b'o' | b'O') => Some(|byte| (b'0'..=b'7').contains(byte)),
            Some(b'b' | b'B') => Some(|byte| matches!(byte, b'0' | b'1')),
            _ => None,
        };
        if let Some(digit) = radix {
            return run(at + 2, digit);
        }
    }
    let mut end = run(at, u8::is_ascii_digit);
    if line.get(end) == Some(&b'.') {
        end = run(end + 1, u8::is_ascii_digit);
    }
    if matches!(line.get(end), Some(b'e' | b'E')) {
        let sign = usize::from(matches!(line.get(end + 1), Some(b'+' | b'-')));
        if line.get(end + 1 + sign).is_some_and(u8::is_ascii_digit) {
            end = run(end + 1 + sign, u8::is_ascii_digit);
        }
    }
    if matches!(line.get(end), Some(b'j' | b'J')) {
        end += 1;
    }
    end
}

/// The operators of three and two bytes; one-byte ones are [`ONE_BYTE_OPERATORS`].
const OPERATORS: &[&[u8]] = &[
    b"**=", b"//=", b">>=", b"<<=", b"**", b"//", b"<<", b">>", b"<=", b">=", b"==", b"!=", b"->",
    b":=", b"+=", b"-=", b"*=", b"/=", b"%=", b"&=", b"|=", b"^=", b"@=",
];
const ONE_BYTE_OPERATORS: &[u8] = b"+-*/%@&|^~<>=!";

/// The width of the operator `rest` starts with, if it starts with one.
pub(crate) fn operator_width(rest: &[u8]) -> Option<usize> {
    OPERATORS
        .iter()
        .find(|operator| rest.starts_with(operator))
        .map(|operator| operator.len())
        .or_else(|| {
            rest.first()
                .filter(|byte| ONE_BYTE_OPERATORS.contains(byte))
                .map(|_| 1)
        })
}

/// The hard keywords; `True`, `False` and `None` are [`is_literal`].
pub(crate) fn is_keyword(word: &[u8]) -> bool {
    matches!(
        word,
        b"and"
            | b"as"
            | b"assert"
            | b"async"
            | b"await"
            | b"break"
            | b"class"
            | b"continue"
            | b"def"
            | b"del"
            | b"elif"
            | b"else"
            | b"except"
            | b"finally"
            | b"for"
            | b"from"
            | b"global"
            | b"if"
            | b"import"
            | b"in"
            | b"is"
            | b"lambda"
            | b"nonlocal"
            | b"not"
            | b"or"
            | b"pass"
            | b"raise"
            | b"return"
            | b"try"
            | b"while"
            | b"with"
            | b"yield"
    )
}

/// The constants coloured as `literal`.
pub(crate) fn is_literal(word: &[u8]) -> bool {
    matches!(
        word,
        b"True" | b"False" | b"None" | b"Ellipsis" | b"NotImplemented" | b"__debug__"
    )
}

/// Whether `word`, the first of a statement and ending at `end`, is a soft keyword there:
/// `match` or `case` heading a block (no operator after it, a `:` ending the line), or `type`
/// naming an alias (`type Pair = ...`, `type Pair[T] = ...`).
pub(crate) fn is_soft_keyword(line: &[u8], word: &[u8], end: usize) -> bool {
    let rest = line.get(end..).unwrap_or_default();
    match word {
        b"match" | b"case" => {
            let next = rest.get(skip_blanks(rest, 0));
            next.is_some_and(|byte| !b":,;=.)]}".contains(byte)) && ends_in_colon(rest)
        }
        b"type" => is_type_alias(rest),
        _ => false,
    }
}

/// Whether the code of `rest` ends in `:` before its comment; quoted text is skipped.
fn ends_in_colon(rest: &[u8]) -> bool {
    let mut last = None;
    let mut quote: Option<u8> = None;
    let mut at = 0;
    while let Some(&byte) = rest.get(at) {
        match quote {
            Some(_) if byte == b'\\' => at += 1,
            Some(open) if byte == open => quote = None,
            Some(_) => {}
            None if byte == b'#' => break,
            None if byte == b'"' || byte == b'\'' => {
                quote = Some(byte);
                last = Some(byte);
            }
            None if !is_blank(byte) => last = Some(byte),
            None => {}
        }
        at += 1;
    }
    last == Some(b':')
}

/// `rest` after `type`: a blank, a name, type parameters in `[ ]`, then `=`.
fn is_type_alias(rest: &[u8]) -> bool {
    if !rest.first().copied().is_some_and(is_blank) {
        return false;
    }
    let name = skip_blanks(rest, 0);
    if !rest.get(name).copied().is_some_and(is_name_start) {
        return false;
    }
    let mut at = skip_blanks(rest, name_end(rest, name));
    if rest.get(at) == Some(&b'[') {
        let mut depth = 0u32;
        let Some(close) = rest[at..].iter().position(|byte| {
            match byte {
                b'[' => depth += 1,
                b']' => depth -= 1,
                _ => {}
            }
            depth == 0
        }) else {
            return false;
        };
        at = skip_blanks(rest, at + close + 1);
    }
    rest.get(at) == Some(&b'=') && rest.get(at + 1) != Some(&b'=')
}

/// The class of a built-in name: functions `function`, types and exceptions `type-name`.
pub(crate) fn builtin_class(word: &[u8]) -> Option<TokenClass> {
    if !word.first().is_some_and(u8::is_ascii_alphabetic) && !word.starts_with(b"__") {
        return None;
    }
    if is_builtin_function(word) {
        Some(TokenClass::Function)
    } else if is_builtin_type(word) || is_builtin_exception(word) {
        Some(TokenClass::TypeName)
    } else {
        None
    }
}

/// The built-in functions, and the built-in classes used as functions and decorators.
fn is_builtin_function(word: &[u8]) -> bool {
    matches!(
        word,
        b"abs"
            | b"aiter"
            | b"all"
            | b"anext"
            | b"any"
            | b"ascii"
            | b"bin"
            | b"breakpoint"
            | b"callable"
            | b"chr"
            | b"classmethod"
            | b"compile"
            | b"delattr"
            | b"dir"
            | b"divmod"
            | b"enumerate"
            | b"eval"
            | b"exec"
            | b"filter"
            | b"format"
            | b"getattr"
            | b"globals"
            | b"hasattr"
            | b"hash"
            | b"help"
            | b"hex"
            | b"id"
            | b"input"
            | b"isinstance"
            | b"issubclass"
            | b"iter"
            | b"len"
            | b"locals"
            | b"map"
            | b"max"
            | b"min"
            | b"next"
            | b"oct"
            | b"open"
            | b"ord"
            | b"pow"
            | b"print"
            | b"property"
            | b"repr"
            | b"reversed"
            | b"round"
            | b"setattr"
            | b"sorted"
            | b"staticmethod"
            | b"sum"
            | b"super"
            | b"vars"
            | b"zip"
            | b"__import__"
    )
}

/// The built-in types.
fn is_builtin_type(word: &[u8]) -> bool {
    matches!(
        word,
        b"bool"
            | b"bytearray"
            | b"bytes"
            | b"complex"
            | b"dict"
            | b"float"
            | b"frozenset"
            | b"int"
            | b"list"
            | b"memoryview"
            | b"object"
            | b"range"
            | b"set"
            | b"slice"
            | b"str"
            | b"tuple"
            | b"type"
    )
}

/// The built-in exceptions and warnings.
fn is_builtin_exception(word: &[u8]) -> bool {
    matches!(
        word,
        b"BaseException"
            | b"BaseExceptionGroup"
            | b"GeneratorExit"
            | b"KeyboardInterrupt"
            | b"SystemExit"
            | b"Exception"
            | b"ArithmeticError"
            | b"FloatingPointError"
            | b"OverflowError"
            | b"ZeroDivisionError"
            | b"AssertionError"
            | b"AttributeError"
            | b"BufferError"
            | b"EOFError"
            | b"ExceptionGroup"
            | b"ImportError"
            | b"ModuleNotFoundError"
            | b"LookupError"
            | b"IndexError"
            | b"KeyError"
            | b"MemoryError"
            | b"NameError"
            | b"UnboundLocalError"
            | b"OSError"
            | b"BlockingIOError"
            | b"ChildProcessError"
            | b"ConnectionError"
            | b"BrokenPipeError"
            | b"ConnectionAbortedError"
            | b"ConnectionRefusedError"
            | b"ConnectionResetError"
            | b"FileExistsError"
            | b"FileNotFoundError"
            | b"InterruptedError"
            | b"IsADirectoryError"
            | b"NotADirectoryError"
            | b"PermissionError"
            | b"ProcessLookupError"
            | b"TimeoutError"
            | b"ReferenceError"
            | b"RuntimeError"
            | b"NotImplementedError"
            | b"PythonFinalizationError"
            | b"RecursionError"
            | b"StopAsyncIteration"
            | b"StopIteration"
            | b"SyntaxError"
            | b"IndentationError"
            | b"TabError"
            | b"SystemError"
            | b"TypeError"
            | b"ValueError"
            | b"UnicodeError"
            | b"UnicodeDecodeError"
            | b"UnicodeEncodeError"
            | b"UnicodeTranslateError"
            | b"EnvironmentError"
            | b"IOError"
            | b"WindowsError"
            | b"Warning"
            | b"BytesWarning"
            | b"DeprecationWarning"
            | b"EncodingWarning"
            | b"FutureWarning"
            | b"ImportWarning"
            | b"PendingDeprecationWarning"
            | b"ResourceWarning"
            | b"RuntimeWarning"
            | b"SyntaxWarning"
            | b"UnicodeWarning"
            | b"UserWarning"
    )
}

#[cfg(test)]
#[path = "words_tests.rs"]
mod tests;
