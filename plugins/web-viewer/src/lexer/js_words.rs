//! JavaScript's and TypeScript's names: what a word is by itself and by what stands around it,
//! and where type arguments end.

use super::scan::{Word, is_ident, is_ident_start};
use super::state::{Frame, JsFrame};
use super::words::{
    JS_BEFORE_EXPRESSION, JS_BEFORE_TYPE, JS_KEYWORDS, JS_LITERALS, JS_SOFT, TS_BEFORE_TYPE,
    TS_KEYWORDS, TS_TYPES,
};
use super::{Lexeme, Lexer};
use crate::types::{Failure, Source, TokenClass};

/// The bytes looked at for type arguments after a `<`.
pub(super) const GENERIC_LOOK: usize = 256;

/// The length of the type arguments `bytes` start with - `<T>`, `<string, Array<number>>` -
/// or 0 when the `<` starts none: what is between is names, `,` `.` `?` `:` `=` `|` `&`,
/// quotes and balanced brackets, on one line; `&&`, `||`, `;` and arithmetic make a
/// comparison of it.
pub(super) fn generic_len(bytes: &[u8]) -> usize {
    if bytes.first() != Some(&b'<') {
        return 0;
    }
    let first = bytes
        .iter()
        .skip(1)
        .find(|byte| !matches!(**byte, b' ' | b'\t'))
        .copied()
        .unwrap_or(0);
    if !(is_ident_start(first) || matches!(first, b'{' | b'[' | b'(' | b'\'' | b'"')) {
        return 0;
    }
    let (mut depth, mut nested, mut prev) = (0usize, 0usize, 0u8);
    for (index, &byte) in bytes.iter().enumerate() {
        match byte {
            b'<' => depth += 1,
            // The `>` of a function type's `=>`.
            b'>' if prev == b'=' => {}
            b'>' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return if nested == 0 { index + 1 } else { 0 };
                }
            }
            b'(' | b'[' | b'{' => nested += 1,
            b')' | b']' | b'}' => {
                if nested == 0 {
                    return 0;
                }
                nested -= 1;
            }
            b'&' | b'|' if prev == byte => return 0,
            b' ' | b'\t' | b',' | b'.' | b'?' | b':' | b'=' | b'\'' | b'"' | b'&' | b'|' => {}
            _ if is_ident(byte) => {}
            _ => return 0,
        }
        prev = byte;
    }
    0
}

/// Whether `word` is a keyword where it stands: a keyword that is also a name (`get`, `of`)
/// is a function where it is called, and TypeScript's `type` a keyword only before a name.
fn is_keyword(word: &Word, ts: bool, called: bool, after: u8) -> bool {
    let keyword = word.is_in(JS_KEYWORDS) || (ts && word.is_in(TS_KEYWORDS));
    if !keyword || (called && word.is_in(JS_SOFT)) {
        return false;
    }
    !(ts && word.is_in(&["type"])) || is_ident_start(after)
}

impl<S: Source> Lexer<'_, S> {
    /// A name, from its first character at `start`.
    pub(super) fn js_word(&mut self, mut frame: JsFrame, start: u32) -> Result<Lexeme, Failure> {
        let word = self.word_ahead(b"")?;
        self.take_word(start, b"")?;
        let ts = frame.dialect.ts();
        let after = self.after_blanks()?;
        // Type arguments right after the name: `Array<string>`, `useState<number>(0)`.
        let mut generic = None;
        if ts && self.peek()? == Some(b'<') && !self.in_generic() {
            let bytes = self.reader.look(GENERIC_LOOK)?;
            let len = generic_len(bytes);
            let call = bytes.get(len) == Some(&b'(');
            if len > 0 {
                generic = Some(call);
                self.scratch.js.generic_end = self.reader.offset() + len as u64;
            }
        }
        let scratch = &mut self.scratch.js;
        let after_dot = std::mem::take(&mut scratch.after_dot);
        let type_next = std::mem::take(&mut scratch.type_next);
        let annot = std::mem::take(&mut scratch.annot);
        let after_case = std::mem::take(&mut scratch.after_case);
        scratch.in_type = false;
        let called = after[0] == b'(';
        let keyed = (after[0] == b':' || (ts && after == *b"?:"))
            && scratch.ternary == 0
            && !after_case
            && !word.is_in(&["default"]);
        let typed = self.in_generic()
            || type_next
            || (annot && ts && (word.is_in(TS_TYPES) || word.starts_upper()));
        frame.regex_ok = false;
        let class = if after_dot {
            called.then_some(TokenClass::Function)
        } else if word.is_in(JS_LITERALS) {
            Some(TokenClass::Literal)
        } else if keyed {
            frame.regex_ok = true;
            Some(TokenClass::Key)
        } else if is_keyword(&word, ts, called, after[0]) {
            frame.regex_ok = word.is_in(JS_BEFORE_EXPRESSION);
            self.scratch.js.type_next =
                word.is_in(JS_BEFORE_TYPE) || (ts && word.is_in(TS_BEFORE_TYPE));
            self.scratch.js.after_case = word.is_in(&["case"]);
            Some(TokenClass::Keyword)
        } else if let Some(call) = generic {
            Some(if call {
                TokenClass::Function
            } else {
                TokenClass::TypeName
            })
        } else if typed {
            Some(TokenClass::TypeName)
        } else if called {
            Some(TokenClass::Function)
        } else if word.is_capitalized() {
            Some(TokenClass::TypeName)
        } else {
            None
        };
        self.scratch.js.in_type = class == Some(TokenClass::TypeName);
        self.set_top(Frame::Js(frame));
        Ok(self.lexeme(class, start))
    }
}
