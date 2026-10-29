//! Stack traces: the frame lines coloured `type-name` as a whole, and which lines go on with the
//! record of an error or a warning for the setting `level_colour = line`.
//!
//! Java and .NET (`<tab>at x.Y.m(Y.java:10)`, `Caused by:`, `Suppressed:`, `... 3 more`),
//! JavaScript (`    at f (/x.js:1:2)`) and Python (`Traceback (most recent call last):`,
//! `  File "x.py", line 3, in m`). Every test looks at a line's start, or once through a frame
//! line it has recognised by its start.

/// Python's first line of a traceback.
const TRACEBACK: &[u8] = b"Traceback (most recent call last):";
/// The last word of an exception's name on the line that names it (`java.io.IOException: x`,
/// `ValueError: x`).
const EXCEPTIONS: [&[u8]; 4] = [b"Error", b"Exception", b"Throwable", b"Warning"];

/// Whether `line` is a line of a stack trace.
pub(super) fn is_frame(line: &[u8]) -> bool {
    let trimmed = line.trim_ascii_start();
    let indented = trimmed.len() < line.len();
    (indented && (call_frame(trimmed) || more_frames(trimmed)))
        || python_frame(trimmed)
        || trimmed.starts_with(b"Caused by:")
        || trimmed.starts_with(b"Suppressed:")
        || trimmed.starts_with(TRACEBACK)
}

/// Whether `line` begins a Python traceback, an error even without a level before it.
pub(super) fn starts_trace(line: &[u8]) -> bool {
    line.trim_ascii_start().starts_with(TRACEBACK)
}

/// Whether `line` goes on with the record before it: a frame, an indented line (the source line
/// of a Python frame, a message over several lines) or the line naming the exception.
pub(super) fn continues(line: &[u8]) -> bool {
    let indented = line
        .first()
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        && !line.trim_ascii_start().is_empty();
    indented || is_frame(line) || names_exception(line)
}

/// `at x.Y.m(Y.java:10)`, `at f (/x.js:1:2)`, `at X.M() in C:\x.cs:line 4`: `at`, a name, and a
/// `(` or a `:` somewhere after it.
fn call_frame(trimmed: &[u8]) -> bool {
    trimmed.strip_prefix(b"at ").is_some_and(|rest| {
        rest.first().is_some_and(|byte| !byte.is_ascii_whitespace())
            && rest.iter().any(|byte| matches!(byte, b'(' | b':'))
    })
}

/// `... 3 more`, `... 12 common frames omitted`.
fn more_frames(trimmed: &[u8]) -> bool {
    trimmed
        .strip_prefix(b"... ")
        .is_some_and(|rest| rest.first().is_some_and(u8::is_ascii_digit))
}

/// `File "x.py", line 3, in m`.
fn python_frame(trimmed: &[u8]) -> bool {
    trimmed.strip_prefix(b"File \"").is_some_and(|rest| {
        rest.windows(b"\", line ".len())
            .any(|window| window == b"\", line ")
    })
}

/// `java.lang.IllegalStateException: boom`, `ValueError: bad`, `java.lang.NullPointerException`:
/// a dotted name ending in one of [`EXCEPTIONS`] at the line's start, then `:` or the line's end.
fn names_exception(line: &[u8]) -> bool {
    let length = line
        .iter()
        .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'$'))
        .count();
    let name = line.get(..length).unwrap_or_default();
    name.first().is_some_and(u8::is_ascii_alphabetic)
        && EXCEPTIONS.iter().any(|last| name.ends_with(last))
        && matches!(line.get(length), None | Some(b':'))
}
