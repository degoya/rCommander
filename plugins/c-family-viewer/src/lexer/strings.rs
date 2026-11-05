//! Strings, inside which the lexer reads text: their openings with every prefix the family
//! writes, how each kind closes, and the fields of interpolated ones - Kotlin's `$name` and
//! `${ }`, C#'s `{ }` with `{{ }}` and format specs, and the `$$` of C#'s raw strings.

use super::frames::{Frame, Str, StrKind};
use super::{Delimiter, Emit, Lexer, MAX_DELIMITER, Mode, words};
use crate::lang::Lang;
use crate::types::TokenClass;

/// What a byte of string text does.
enum Next {
    /// The string closes before this offset.
    Close(usize),
    /// The text goes on from this offset.
    Skip(usize),
}

impl<E: Emit> Lexer<'_, E> {
    /// A string opens at `start`, its opening `width` bytes wide.
    pub(super) fn open_string(&mut self, start: usize, width: usize, open: Str) {
        self.mode = Mode::Expr;
        self.pos = (start + width).min(self.line.len());
        if !self.frames.push(Frame::Str(open)) {
            self.rest_of_line(start, TokenClass::String);
            return;
        }
        self.emit
            .opened(start, u8::try_from(width).unwrap_or(u8::MAX));
        self.run_from = Some(start);
    }

    /// `"` at `quote`, after a prefix from `start` (`L`, `u8`, `b`, C#'s `$` signs, `dollars`
    /// of them): one quote, or three and more opening a string over lines where the language
    /// has them - C#'s raw string closes with as many quotes as it opened with.
    pub(super) fn open_quote(&mut self, start: usize, quote: usize, dollars: u8) {
        let run = words::run_of(self.line, quote, b'"');
        let dollars = if self.lang == Lang::Kotlin {
            1
        } else {
            dollars
        };
        let (kind, fence) = if self.lang.triple_quotes() && run >= 3 {
            let fence = if self.lang == Lang::CSharp { run } else { 3 };
            (StrKind::Triple, fence)
        } else {
            (StrKind::Quoted, 1)
        };
        let open = Str {
            kind,
            fence: u8::try_from(fence).unwrap_or(u8::MAX),
            dollars,
        };
        self.open_string(start, quote - start + fence, open);
    }

    /// The name `start..end` prefixes the string or character right after it: `L"`, `u8'`,
    /// `R"x(` (C, C++); `b"`, `c"`, `b'`, `r#"`, `br"`, and the raw name `r#type` (Rust).
    /// Returns whether it did and the lexer moved past it.
    pub(super) fn prefixed(&mut self, start: usize, end: usize) -> bool {
        let line = self.line;
        let word = &line[start..end];
        let encoding = matches!(word, b"L" | b"u" | b"U" | b"u8");
        match (self.lang, self.byte(end)) {
            (Lang::C | Lang::Cpp, Some(b'"')) if encoding => self.open_quote(start, end, 0),
            (Lang::C | Lang::Cpp, Some(b'\'')) if encoding => self.char_literal(start, end),
            (Lang::Cpp, Some(b'"')) if matches!(word, b"R" | b"LR" | b"uR" | b"UR" | b"u8R") => {
                self.cpp_raw(start, end);
            }
            (Lang::Rust, Some(b'"')) if matches!(word, b"b" | b"c") => {
                self.open_quote(start, end, 0);
            }
            (Lang::Rust, Some(b'\'')) if word == b"b" => self.char_literal(start, end),
            (Lang::Rust, Some(b'"' | b'#')) if matches!(word, b"r" | b"br" | b"cr") => {
                return self.rust_raw(start, end, word == b"r");
            }
            _ => return false,
        }
        true
    }

    /// C++'s `R"delim(`, its quote at `quote`; a delimiter longer than the standard's 16 bytes or
    /// without its `(` makes a plain string.
    fn cpp_raw(&mut self, start: usize, quote: usize) {
        let line = self.line;
        let from = quote + 1;
        let paren = line[from..]
            .iter()
            .take(MAX_DELIMITER + 1)
            .position(|&byte| matches!(byte, b'(' | b')' | b'\\' | b'"' | b' ' | b'\t'));
        match paren {
            Some(length) if line[from + length] == b'(' => {
                self.delimiter = Delimiter::new(&line[from..from + length]);
                self.open_string(
                    start,
                    quote + length + 2 - start,
                    Str::plain(StrKind::CppRaw),
                );
            }
            _ => self.open_quote(start, quote, 0),
        }
    }

    /// Rust's `r"`, `r#"`, `br##"`, `cr#"` with `end` after the prefix; `r#name` (`raw`: the
    /// prefix is `r`) is a name that is no keyword. False for anything else.
    fn rust_raw(&mut self, start: usize, end: usize, raw: bool) -> bool {
        let hashes = words::run_of(self.line, end, b'#');
        match self.byte(end + hashes) {
            Some(b'"') => {
                let open = Str {
                    fence: u8::try_from(hashes).unwrap_or(u8::MAX),
                    ..Str::plain(StrKind::RustRaw)
                };
                self.open_string(start, end + hashes + 1 - start, open);
                true
            }
            Some(byte) if raw && hashes == 1 && words::is_name_start(self.lang, byte) => {
                self.pos = words::name_end(self.lang, self.line, end + 1);
                self.mode = Mode::Expr;
                true
            }
            _ => false,
        }
    }

    /// C#'s `$`: `$"`, `$$"""`, `$@"`; a `$` before nothing of these is skipped.
    pub(super) fn dollar(&mut self, at: usize) {
        let dollars = words::run_of(self.line, at, b'$');
        let quote = at + dollars;
        match self.byte(quote) {
            Some(b'"') => self.open_quote(at, quote, u8::try_from(dollars).unwrap_or(u8::MAX)),
            Some(b'@') if dollars == 1 && self.byte(quote + 1) == Some(b'"') => {
                let open = Str {
                    dollars: 1,
                    ..Str::plain(StrKind::Verbatim)
                };
                self.open_string(at, 3, open);
            }
            _ => self.pos = quote,
        }
    }

    /// Text of a string up to its closer, a field, or the line's end.
    pub(super) fn string(&mut self, open: Str) {
        let start = self.run_from.take().unwrap_or(self.pos);
        let fields = open.dollars > 0;
        let mut at = self.pos;
        while let Some(byte) = self.byte(at) {
            match byte {
                b'\\' if self.escapes(open) => {
                    if at + 1 == self.line.len() {
                        // C and C++ join the next line to this one.
                        self.string_continued = self.lang.splices();
                    }
                    at += 2;
                    continue;
                }
                b'$' if fields && self.lang == Lang::Kotlin => {
                    if self.template(start, at) {
                        return;
                    }
                    at += 1;
                    continue;
                }
                b'{' if fields && self.lang == Lang::CSharp => {
                    match self.interpolation(start, at, open) {
                        Some(next) => at = next,
                        None => return,
                    }
                    continue;
                }
                // `}}` is a brace.
                b'}' if fields && self.lang == Lang::CSharp => {
                    at += words::run_of(self.line, at, b'}').max(1);
                    continue;
                }
                _ => {}
            }
            match self.quote_step(at, open) {
                Next::Close(end) => {
                    self.push(start, end, TokenClass::String);
                    self.pos = end;
                    self.emit.closed();
                    self.frames.pop();
                    if open.kind == StrKind::CppRaw {
                        self.delimiter = Delimiter::default();
                    }
                    return;
                }
                Next::Skip(next) => at = next,
            }
        }
        self.rest_of_line(start, TokenClass::String);
    }

    /// Backslash escapes: in `"..."`, and in Java's text blocks.
    fn escapes(&self, open: Str) -> bool {
        open.kind == StrKind::Quoted || (open.kind == StrKind::Triple && self.lang == Lang::Java)
    }

    /// Whether the string closes at `at`, or how far its text goes on.
    fn quote_step(&self, at: usize, open: Str) -> Next {
        let line = self.line;
        let byte = line[at];
        match open.kind {
            StrKind::Quoted if byte == b'"' => Next::Close(at + 1),
            StrKind::Backtick if byte == b'`' => Next::Close(at + 1),
            StrKind::Triple if byte == b'"' => {
                // Kotlin's `""""` ends with a quote of text; C#'s closer is the opener's width.
                let run = words::run_of(line, at, b'"');
                if run >= usize::from(open.fence.max(3)) {
                    Next::Close(at + run)
                } else {
                    Next::Skip(at + run)
                }
            }
            StrKind::Verbatim if byte == b'"' => {
                if self.byte(at + 1) == Some(b'"') {
                    Next::Skip(at + 2)
                } else {
                    Next::Close(at + 1)
                }
            }
            StrKind::RustRaw if byte == b'"' => {
                let fence = usize::from(open.fence);
                if words::run_of(line, at + 1, b'#') >= fence {
                    Next::Close(at + 1 + fence)
                } else {
                    Next::Skip(at + 1)
                }
            }
            StrKind::CppRaw if byte == b')' => {
                let delimiter = self.delimiter.bytes();
                let after = at + 1 + delimiter.len();
                if line[at + 1..].starts_with(delimiter) && self.byte(after) == Some(b'"') {
                    Next::Close(after + 1)
                } else {
                    Next::Skip(at + 1)
                }
            }
            _ => Next::Skip(at + 1),
        }
    }

    /// Kotlin's `$` at `at` in a string's text from `start`: `${` opens a field, `$name` is a
    /// name in the text. False when it is neither, a `$` of the text.
    fn template(&mut self, start: usize, at: usize) -> bool {
        match self.byte(at + 1) {
            Some(b'{') if !self.frames.full() => {
                self.push(start, at, TokenClass::String);
                self.open_field(at, 2, 1);
                true
            }
            Some(byte) if words::is_name_start(self.lang, byte) => {
                let line = self.line;
                let end = words::name_end(self.lang, line, at + 1);
                self.push(start, at, TokenClass::String);
                self.push(at, at + 1, TokenClass::Punctuation);
                if let Some(class) = self.lang.classify(&line[at + 1..end]) {
                    self.push(at + 1, end, class);
                }
                self.pos = end;
                true
            }
            _ => false,
        }
    }

    /// C#'s `{` at `at` in a string's text from `start`: `Some` of where the text goes on when
    /// the braces are text (`{{`; in a raw string fewer than its `$` signs), `None` when a field
    /// opened.
    fn interpolation(&mut self, start: usize, at: usize, open: Str) -> Option<usize> {
        let run = words::run_of(self.line, at, b'{');
        let (field, width) = if open.kind == StrKind::Triple {
            let need = usize::from(open.dollars);
            if run < need {
                return Some(at + run);
            }
            // The last `$` count of a longer run open the field; those before are text.
            (at + run - need, need)
        } else if run >= 2 {
            return Some(at + 2);
        } else {
            (at, 1)
        };
        if self.frames.full() {
            return Some(at + run);
        }
        self.push(start, field, TokenClass::String);
        self.open_field(field, width, u8::try_from(width).unwrap_or(u8::MAX));
        None
    }

    /// The `width` braces at `at` open a field that `close` braces close.
    fn open_field(&mut self, at: usize, width: usize, close: u8) {
        self.push(at, at + width, TokenClass::Punctuation);
        self.pos = at + width;
        self.frames.push(Frame::Field { depth: 0, close });
        self.mode = Mode::Expr;
    }

    /// C#'s format spec up to its field's `close` braces.
    pub(super) fn spec(&mut self, close: u8) {
        let start = self.pos;
        let width = usize::from(close.max(1));
        let mut at = self.pos;
        while let Some(byte) = self.byte(at) {
            if byte != b'}' {
                at += 1;
                continue;
            }
            let run = words::run_of(self.line, at, b'}');
            if run >= width {
                self.push(start, at, TokenClass::String);
                self.push(at, at + width, TokenClass::Punctuation);
                self.pos = at + width;
                self.frames.pop();
                self.mode = Mode::Expr;
                return;
            }
            at += run;
        }
        self.rest_of_line(start, TokenClass::String);
    }
}

#[cfg(test)]
#[path = "strings_tests.rs"]
mod tests;
