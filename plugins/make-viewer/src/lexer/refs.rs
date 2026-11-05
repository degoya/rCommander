//! References: `$(NAME)` and `${NAME}` whole as `key`, with what nests in their name; the
//! automatic variables `$@ $< $^ $? $* $+ $|` and one-letter names; `$$`, a dollar for the shell,
//! with the shell variable after it; and the calls of make's functions, `$(patsubst %.c,%.o,...)`,
//! whose brackets and commas are punctuation, whose name is a function and whose arguments are
//! read for references.
//!
//! Calls nest without recursion: the open calls are a stack of at most [`MAX_CALLS`]; a call
//! deeper than that is read as one reference, so a line of a million `$(if ` stays linear.

use super::{Emit, Lexer, is_blank, words};
use crate::types::TokenClass;

/// The most calls open inside each other that keep their own colours.
const MAX_CALLS: usize = 32;

/// A byte of a function's name.
fn is_function_name(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte == b'-'
}

/// The bracket that closes `open`.
fn closer(open: u8) -> u8 {
    if open == b'{' { b'}' } else { b')' }
}

/// An open call: the bracket that closes it and the plain brackets of its kind open inside it.
#[derive(Clone, Copy, Default)]
struct Call {
    close: u8,
    plain: usize,
}

impl<E: Emit> Lexer<'_, E> {
    /// The reference at `at` (a `$`), coloured; the first byte after it.
    pub(crate) fn reference(&mut self, at: usize) -> usize {
        match self.byte(at + 1) {
            None => {
                self.push(at, at + 1, TokenClass::Key);
                at + 1
            }
            Some(b'$') => {
                let end = self.run(at + 2, |byte| byte.is_ascii_alphanumeric() || byte == b'_');
                self.push(at, end, TokenClass::Key);
                end
            }
            Some(open @ (b'(' | b'{')) => match self.called(at) {
                Some(name_end) => self.call(at, closer(open), name_end),
                None => self.whole(at, open),
            },
            Some(byte) if byte.is_ascii() && !is_blank(byte) => {
                self.push(at, at + 2, TokenClass::Key);
                at + 2
            }
            Some(_) => {
                self.push(at, at + 1, TokenClass::Key);
                at + 1
            }
        }
    }

    /// Whether `$(` at `at` calls a function - its name, then a blank: the end of its name.
    fn called(&self, at: usize) -> Option<usize> {
        let end = self.run(at + 2, is_function_name);
        let name = self.line.get(at + 2..end)?;
        (words::is_function(name) && self.byte(end).is_some_and(is_blank)).then_some(end)
    }

    /// The reference `$(...)` opened with `open` at `at`, whole as `key`: the first byte after it.
    fn whole(&mut self, at: usize, open: u8) -> usize {
        let end = self.matching(at + 2, open);
        self.push(at, end, TokenClass::Key);
        end
    }

    /// The first byte after the bracket that closes `open` from `from`, or the line's end.
    fn matching(&self, from: usize, open: u8) -> usize {
        let close = closer(open);
        let mut depth = 1usize;
        let mut at = from;
        while let Some(byte) = self.byte(at) {
            at += 1;
            if byte == open {
                depth += 1;
            } else if byte == close {
                depth -= 1;
                if depth == 0 {
                    return at;
                }
            }
        }
        at
    }

    /// The call opened at `at` with `close`, its name ending at `name_end`: on to its closing
    /// bracket, or the line's end.
    fn call(&mut self, at: usize, close: u8, name_end: usize) -> usize {
        let mut calls = [Call::default(); MAX_CALLS];
        let mut depth = 0usize;
        let mut pos = at;
        let mut name_end = name_end;
        loop {
            // A call opens at `pos`, its name ends at `name_end`.
            self.push(pos, pos + 2, TokenClass::Punctuation);
            self.push(pos + 2, name_end, TokenClass::Function);
            calls[depth] = Call {
                close: if depth == 0 {
                    close
                } else {
                    closer(self.line[pos + 1])
                },
                plain: 0,
            };
            depth += 1;
            pos = name_end;
            // The arguments, until a nested call opens or the outermost closes.
            loop {
                let Some(byte) = self.byte(pos) else {
                    return pos;
                };
                let top = &mut calls[depth - 1];
                let open = if top.close == b'}' { b'{' } else { b'(' };
                if byte == b'$' {
                    let bracket = self
                        .byte(pos + 1)
                        .filter(|byte| matches!(*byte, b'(' | b'{'));
                    match (bracket, self.called(pos)) {
                        (Some(_), Some(end)) if depth < MAX_CALLS => {
                            name_end = end;
                            break;
                        }
                        // Too deep for its own colours: one reference, never a recursion.
                        (Some(open), _) => pos = self.whole(pos, open),
                        (None, _) => pos = self.reference(pos),
                    }
                } else if byte == open {
                    top.plain += 1;
                    pos += 1;
                } else if byte == top.close && top.plain > 0 {
                    top.plain -= 1;
                    pos += 1;
                } else if byte == top.close {
                    self.push(pos, pos + 1, TokenClass::Punctuation);
                    pos += 1;
                    depth -= 1;
                    if depth == 0 {
                        return pos;
                    }
                } else if byte == b',' {
                    self.push(pos, pos + 1, TokenClass::Punctuation);
                    pos += 1;
                } else {
                    pos += 1;
                }
            }
        }
    }
}
