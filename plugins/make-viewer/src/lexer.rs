//! The Makefile lexer: one line at a time, in the state the line before left, for both passes
//! (the line index of `open` and the tokens of the lines on screen). It reads the UTF-8 text the
//! Lister shows, so every offset is a byte of it.
//!
//! A line is a directive (`include`, `ifeq`, `define` ...), an assignment (`=`, `:=`, `::=`,
//! `?=`, `+=`, `!=`), a rule (`targets: prerequisites`, `::`, static patterns, a recipe after
//! `;`), a recipe line (a tab first) or a comment. What reaches from one line into the next is the
//! [`LineState`]: a line ending in a backslash goes on as what it was - a recipe, a value, the
//! prerequisites, a comment - and the body of a `define` runs to its `endef`.
//!
//! The lexer colours, it does not parse, and never fails. Recipes get the shell viewer's colours
//! as far as they need no grammar of their own ([`recipe`]); `$(...)` and `${...}` everywhere get
//! theirs ([`refs`]). Every decision is on ASCII bytes, so every token sits on character
//! boundaries.

mod directives;
mod recipe;
mod refs;
mod words;

use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;

/// What a line ending in a backslash goes on as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Continued {
    /// Nothing: the line stands on its own.
    #[default]
    No,
    Comment,
    /// A value: of an assignment, an `include`, a condition.
    Value,
    /// A rule's prerequisites.
    Rule,
    Recipe,
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) continued: Continued,
    /// Inside `define` ... `endef`, this many deep.
    pub(crate) define: u8,
}

/// What the lexer reports besides the state: its tokens.
pub(crate) trait Emit {
    /// The token `start..end` of the line.
    fn token(&mut self, _start: usize, _end: usize, _class: TokenClass) {}
}

/// Only the state.
impl Emit for () {}

/// The tokens of one line, at most [`MAX_TOKENS_PER_LINE`].
impl Emit for Vec<Token> {
    fn token(&mut self, start: usize, end: usize, class: TokenClass) {
        let (Ok(at), Ok(len)) = (
            u32::try_from(start),
            u32::try_from(end.saturating_sub(start)),
        ) else {
            return;
        };
        if len > 0 && self.len() < MAX_TOKENS_PER_LINE {
            self.push(Token {
                start: at,
                len,
                class,
            });
        }
    }
}

/// Blank: a space, a tab, a form feed, a stray `\r`.
pub(crate) fn is_blank(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | 0x0B | 0x0C | b'\r')
}

/// Whether `line` ends in a backslash that is not itself escaped.
fn ends_continued(line: &[u8]) -> bool {
    line.iter().rev().take_while(|byte| **byte == b'\\').count() % 2 == 1
}

/// Lexes `line` (UTF-8, without its line break and a `\r` before it) from `state`, reporting to
/// `emit`. Returns the state the next line starts in.
pub(crate) fn lex_line(line: &[u8], state: LineState, emit: &mut impl Emit) -> LineState {
    let mut lexer = Lexer {
        line,
        define: state.define,
        emit,
    };
    let at_end = match state.continued {
        Continued::No if state.define > 0 => lexer.define_body(),
        Continued::No if line.first() == Some(&b'\t') => lexer.recipe(1),
        Continued::No => lexer.statement(0),
        Continued::Comment => lexer.comment(0),
        Continued::Value => lexer.value(0),
        Continued::Rule => lexer.prerequisites(0),
        Continued::Recipe => lexer.recipe_goes_on(),
    };
    let continued = if ends_continued(line) {
        at_end
    } else {
        Continued::No
    };
    LineState {
        continued,
        define: lexer.define,
    }
}

/// An assignment's operator found, or a rule's colon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operator {
    /// `=`, `:=`, `::=`, `:::=`, `?=`, `+=`, `!=`: its length.
    Assign(usize),
    /// `:` or `::`: its length.
    Rule(usize),
}

pub(crate) struct Lexer<'a, E: Emit> {
    pub(crate) line: &'a [u8],
    define: u8,
    emit: &'a mut E,
}

impl<E: Emit> Lexer<'_, E> {
    pub(crate) fn byte(&self, at: usize) -> Option<u8> {
        self.line.get(at).copied()
    }

    pub(crate) fn push(&mut self, start: usize, end: usize, class: TokenClass) {
        self.emit.token(start, end, class);
    }

    /// The end of the run from `at` of bytes `keep` takes.
    pub(crate) fn run(&self, at: usize, keep: impl Fn(u8) -> bool) -> usize {
        at + self
            .line
            .get(at..)
            .unwrap_or_default()
            .iter()
            .take_while(|byte| keep(**byte))
            .count()
    }

    pub(crate) fn skip_blanks(&self, at: usize) -> usize {
        self.run(at, is_blank)
    }

    /// The end of the word at `at`: up to a blank, or the end of the line.
    fn word_end(&self, at: usize) -> usize {
        self.run(at, |byte| !is_blank(byte))
    }

    /// The comment from `at` to the line's end.
    pub(crate) fn comment(&mut self, at: usize) -> Continued {
        self.push(at, self.line.len(), TokenClass::Comment);
        Continued::Comment
    }

    /// A line at the top: directives first, then an assignment, a rule, or a line of text.
    fn statement(&mut self, from: usize) -> Continued {
        let mut at = self.skip_blanks(from);
        let mut prefixed = false;
        loop {
            match self.byte(at) {
                None => return Continued::No,
                Some(b'#') => return self.comment(at),
                Some(_) => {}
            }
            let end = self.run(at, |byte| byte.is_ascii_alphabetic() || byte == b'-');
            let word = self.line.get(at..end).unwrap_or_default();
            let stands = self
                .byte(end)
                .is_none_or(|byte| is_blank(byte) || byte == b'(');
            if !stands || !words::is_directive(word) {
                break;
            }
            self.push(at, end, TokenClass::Keyword);
            let next = self.skip_blanks(end);
            match word {
                b"define" => return self.define(next),
                b"endef" => return self.value(next),
                b"ifeq" | b"ifneq" => return self.condition(next),
                b"ifdef" | b"ifndef" | b"undefine" => return self.names(next),
                b"else" => at = next,
                _ if words::is_prefix(word) => {
                    prefixed = true;
                    at = next;
                }
                _ => return self.value(next),
            }
        }
        match self.find_operator(at) {
            Some((op, Operator::Assign(len))) => self.assignment(at, op, len),
            Some((colon, Operator::Rule(len))) => self.rule(at, colon, len),
            // `export NAME`: names alone.
            None if prefixed => self.names(at),
            None => self.value(at),
        }
    }

    /// The first assignment operator or rule colon from `at` outside `$(...)`, before a comment.
    fn find_operator(&self, from: usize) -> Option<(usize, Operator)> {
        let mut depth = 0usize;
        let mut at = from;
        while let Some(byte) = self.byte(at) {
            let next = self.byte(at + 1);
            match byte {
                b'$' if matches!(next, Some(b'(' | b'{')) => {
                    depth += 1;
                    at += 2;
                    continue;
                }
                b'$' | b'\\' => {
                    at += 2;
                    continue;
                }
                b'(' | b'{' if depth > 0 => depth += 1,
                b')' | b'}' if depth > 0 => depth -= 1,
                _ if depth > 0 => {}
                b'#' => return None,
                b'=' => return Some((at, Operator::Assign(1))),
                b'?' | b'+' | b'!' if next == Some(b'=') => {
                    return Some((at, Operator::Assign(2)));
                }
                b':' => {
                    let colons = self.run(at, |byte| byte == b':') - at;
                    return Some(if self.byte(at + colons) == Some(b'=') {
                        (at, Operator::Assign(colons + 1))
                    } else {
                        (at, Operator::Rule(colons.min(2)))
                    });
                }
                _ => {}
            }
            at += 1;
        }
        None
    }

    /// Directives that may stand before an assignment after a rule's colon (`export`,
    /// `override`, `private`): coloured, the first byte after them returned.
    fn prefixes(&mut self, from: usize) -> usize {
        let mut at = self.skip_blanks(from);
        loop {
            let end = self.word_end(at);
            if end == self.line.len() || !words::is_prefix(&self.line[at..end]) {
                return at;
            }
            self.push(at, end, TokenClass::Keyword);
            at = self.skip_blanks(end);
        }
    }

    /// `name op value`: the name's words as `key`, the operator, the value.
    fn assignment(&mut self, from: usize, op: usize, len: usize) -> Continued {
        self.names_until(from, op);
        self.push(op, op + len, TokenClass::Operator);
        self.value(op + len)
    }

    /// `targets: prerequisites`, a target-specific assignment after the colon, or a static
    /// pattern's second colon.
    fn rule(&mut self, from: usize, colon: usize, len: usize) -> Continued {
        let mut at = from;
        while at < colon {
            match self.byte(at) {
                Some(b'$') => at = self.reference(at),
                Some(byte) if is_blank(byte) => at += 1,
                _ => {
                    let end = self
                        .run(at, |byte| !is_blank(byte) && byte != b'$')
                        .min(colon);
                    let class = if words::is_special_target(&self.line[at..end]) {
                        TokenClass::Keyword
                    } else {
                        TokenClass::Function
                    };
                    self.push(at, end, class);
                    at = end;
                }
            }
        }
        self.push(colon, colon + len, TokenClass::Operator);
        let rest = colon + len;
        match self.find_operator(rest) {
            Some((op, Operator::Assign(len))) => {
                let name = self.prefixes(rest);
                self.assignment(name, op, len)
            }
            _ => self.prerequisites(rest),
        }
    }

    /// Prerequisites from `from`: references, `|` before the order-only ones, a static pattern's
    /// `:`, a recipe after `;`, a comment.
    fn prerequisites(&mut self, from: usize) -> Continued {
        let mut at = from;
        while let Some(byte) = self.byte(at) {
            match byte {
                b'$' => at = self.reference(at),
                b'#' => return self.comment(at),
                b'\\' => at += 2,
                b'|' | b':' => {
                    self.push(at, at + 1, TokenClass::Operator);
                    at += 1;
                }
                b';' => {
                    self.push(at, at + 1, TokenClass::Punctuation);
                    return self.recipe(at + 1);
                }
                _ => at += 1,
            }
        }
        Continued::Rule
    }

    /// A value from `from`: references and a comment; every other byte is the value's own.
    fn value(&mut self, from: usize) -> Continued {
        let mut at = from;
        while let Some(byte) = self.byte(at) {
            match byte {
                b'$' => at = self.reference(at),
                b'#' => return self.comment(at),
                b'\\' => at += 2,
                _ => at += 1,
            }
        }
        Continued::Value
    }

    /// Names from `from` (`ifdef NAME`, `export A B`): each word `key`, and a comment.
    fn names(&mut self, from: usize) -> Continued {
        let end = self.find_comment(from).unwrap_or(self.line.len());
        self.names_until(from, end);
        if end < self.line.len() {
            return self.comment(end);
        }
        Continued::Value
    }

    /// The words from `from` to `end` as `key`, references within them as theirs.
    fn names_until(&mut self, from: usize, end: usize) {
        let mut at = from;
        while at < end {
            match self.byte(at) {
                Some(b'$') => at = self.reference(at),
                Some(byte) if is_blank(byte) => at += 1,
                _ => {
                    let stop = self
                        .run(at, |byte| !is_blank(byte) && byte != b'$')
                        .min(end);
                    self.push(at, stop, TokenClass::Key);
                    at = stop.max(at + 1);
                }
            }
        }
    }

    /// The first `#` from `from` outside a reference.
    fn find_comment(&self, from: usize) -> Option<usize> {
        let mut depth = 0usize;
        let mut at = from;
        while let Some(byte) = self.byte(at) {
            match byte {
                b'$' if matches!(self.byte(at + 1), Some(b'(' | b'{')) => {
                    depth += 1;
                    at += 1;
                }
                b'\\' => at += 1,
                b'(' | b'{' if depth > 0 => depth += 1,
                b')' | b'}' if depth > 0 => depth -= 1,
                b'#' if depth == 0 => return Some(at),
                _ => {}
            }
            at += 1;
        }
        None
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
