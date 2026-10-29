//! Heredocs: the marker after `<<` or `<<-` as `keyword`, the lines up to the marker's line as
//! `string` - with `$name` and `${...}` as `key` when the marker was not quoted - and the
//! marker's line as `keyword` again.

use super::state::{Fnv, Heredoc};
use super::words;
use super::{Lexeme, Lexer};
use crate::types::{Failure, Source, TokenClass};

/// The bytes of a line looked at to tell whether it is the marker's: a longer line is not.
const BODY_LOOK: usize = 1024;

impl<S: Source> Lexer<'_, S> {
    /// The word after `<<` or `<<-` (`strip`): the marker, quoted (`'EOF'`, `"EOF"`, `\EOF`)
    /// or not. The heredoc's lines start on the next line.
    pub(super) fn heredoc_marker(&mut self, strip: bool, start: u32) -> Result<Lexeme, Failure> {
        self.in_word = true;
        let mut hash = Fnv::start();
        let mut len = 0u16;
        let mut literal = false;
        let mut quote: Option<u8> = None;
        while !self.full(start) && !self.at_line_end()? {
            let Some(byte) = self.peek()? else {
                break;
            };
            match (quote, byte) {
                (Some(open), _) if byte == open => {
                    quote = None;
                    self.take()?;
                    continue;
                }
                (None, b'\'' | b'"') => {
                    quote = Some(byte);
                    literal = true;
                    self.take()?;
                    continue;
                }
                (None, _) if words::is_meta(byte) => break,
                (None, b'\\') => {
                    literal = true;
                    self.take()?;
                    if self.at_line_end()? {
                        break;
                    }
                }
                _ => {}
            }
            len = len.saturating_add(self.take_hashed(&mut hash)?);
        }
        if self.pending.is_none() {
            self.pending = Some(Heredoc {
                hash: hash.finish(),
                len,
                strip_tabs: strip,
                literal,
            });
        }
        Ok(self.lexeme(Some(TokenClass::Keyword), start))
    }

    /// Takes one character into `hash`; its bytes in the source.
    fn take_hashed(&mut self, hash: &mut Fnv) -> Result<u16, Failure> {
        let mut bytes = [0u8; 4];
        let raw = {
            let ahead = self.reader.look(4)?;
            let (raw, _) = crate::utf8::unit(ahead);
            bytes[..raw].copy_from_slice(&ahead[..raw]);
            raw
        };
        bytes[..raw].iter().for_each(|byte| hash.add(*byte));
        self.take()?;
        Ok(u16::try_from(raw).unwrap_or(u16::MAX))
    }

    /// A line inside a heredoc starts: its marker's line closes the heredoc and is a `keyword`;
    /// any other line is the heredoc's text.
    pub(super) fn body_start(&mut self) -> Result<Option<Lexeme>, Failure> {
        let Some(heredoc) = self.state.heredoc else {
            return Ok(None);
        };
        let (tabs, marker) = {
            let head = self.reader.look(BODY_LOOK)?;
            let line = match head.iter().position(|byte| *byte == b'\n') {
                Some(end) => Some(&head[..end]),
                None if head.len() < BODY_LOOK => Some(head),
                None => None,
            };
            match line {
                Some(line) => {
                    let line = line.strip_suffix(b"\r").unwrap_or(line);
                    let tabs = if heredoc.strip_tabs {
                        line.iter().take_while(|byte| **byte == b'\t').count()
                    } else {
                        0
                    };
                    let candidate = &line[tabs..];
                    let marker = candidate.len() == usize::from(heredoc.len)
                        && Fnv::of(candidate) == heredoc.hash;
                    (tabs, marker.then_some(candidate.len()))
                }
                None => (0, None),
            }
        };
        let Some(marker) = marker else {
            self.in_body = true;
            return Ok(None);
        };
        self.state.heredoc = None;
        self.take_n(tabs)?;
        let start = self.col;
        let end = self.reader.offset() + marker as u64;
        self.take_to(end)?;
        Ok((self.col > start).then(|| self.lexeme(Some(TokenClass::Keyword), start)))
    }

    /// A heredoc's text on one line, as `string`, its expansions as `key` unless the marker was
    /// quoted.
    pub(super) fn body(&mut self, start: u32) -> Result<Lexeme, Failure> {
        let literal = self.state.heredoc.is_none_or(|heredoc| heredoc.literal);
        if !literal && self.body_expands()? {
            return self.body_variable(start);
        }
        while !self.full(start) && !self.at_line_end()? {
            match self.peek()? {
                Some(b'$') if !literal && self.body_expands()? => break,
                Some(b'\\') if !literal => self.escape()?,
                _ => {
                    self.take()?;
                }
            }
        }
        Ok(self.lexeme(Some(TokenClass::String), start))
    }

    /// Whether a `$` is next and expands in a heredoc's text: `$name`, `$1`, `$@`, `${`.
    fn body_expands(&mut self) -> Result<bool, Failure> {
        Ok(self.peek()? == Some(b'$')
            && self
                .peek_at(1)?
                .is_some_and(|byte| byte != b'(' && words::expands(byte)))
    }

    /// `$name`, `$1`, `$@` or `${...}` in a heredoc's text, to its `}` on the same line.
    fn body_variable(&mut self, start: u32) -> Result<Lexeme, Failure> {
        self.take()?;
        match self.take()? {
            Some(b'{') => {
                while !self.full(start) && !self.at_line_end()? {
                    if self.take()? == Some(b'}') {
                        break;
                    }
                }
            }
            Some(byte) if words::name_start(byte) => {
                while self.peek()?.is_some_and(words::name_byte) && !self.full(start) {
                    self.take()?;
                }
            }
            _ => {}
        }
        Ok(self.lexeme(Some(TokenClass::Key), start))
    }
}
