//! The scalars of the lexer: plain, quoted and block, and the rest of a line.

use super::{Block, Kind, Lexer, Problem, ends_indicator};
use crate::reader::Bytes;
use crate::types::Failure;

impl<B: Bytes> Lexer<B> {
    /// Takes the rest of the line; the end after its last byte that is not blank.
    pub(super) fn take_line_end(&mut self) -> Result<u64, Failure> {
        let mut end = self.bytes.offset();
        while let Some(byte) = self.bytes.peek()? {
            if byte == b'\n' {
                break;
            }
            self.take(byte);
            if !matches!(byte, b' ' | b'\t' | b'\r') {
                end = self.bytes.offset();
            }
        }
        Ok(end)
    }

    /// Takes the rest of an anchor, alias, tag or junk: up to a blank or, in a flow collection,
    /// a flow indicator.
    pub(super) fn word(&mut self) -> Result<(), Failure> {
        let flow = self.state.flow > 0;
        while let Some(byte) = self.bytes.peek()? {
            if ends_indicator(Some(byte), flow) {
                break;
            }
            self.take(byte);
        }
        Ok(())
    }

    /// A plain scalar, up to the line's end, a `:` indicator, a comment or, in a flow
    /// collection, a flow indicator; its end leaves the blanks after it out.
    pub(super) fn plain(&mut self) -> Result<u64, Failure> {
        let flow = self.state.flow > 0;
        let mut end = self.bytes.offset();
        let mut spaced = false;
        while let Some(byte) = self.bytes.peek()? {
            match byte {
                b'\n' => break,
                b' ' | b'\t' | b'\r' => {
                    self.take(byte);
                    spaced = true;
                    continue;
                }
                b'#' if spaced => break,
                b',' | b'[' | b']' | b'{' | b'}' if flow => break,
                b':' if self.indicator(byte)? => break,
                _ => {}
            }
            self.take(byte);
            end = self.bytes.offset();
            spaced = false;
        }
        Ok(end)
    }

    /// A quoted scalar from its quote (`continued`: from the start of a further line) to its
    /// closing quote or the line's end, where it stays open in the state.
    pub(super) fn quoted(&mut self, quote: u8, continued: bool) -> Result<(Kind, u64), Failure> {
        if !continued {
            self.take(quote);
        }
        let mut end = self.bytes.offset();
        let mut bad = false;
        loop {
            let Some(byte) = self.bytes.peek()? else {
                self.state.quote = quote;
                break;
            };
            match byte {
                b'\n' => {
                    self.state.quote = quote;
                    break;
                }
                b'\r' => {
                    self.take(byte);
                    continue;
                }
                b'\\' if quote == b'"' => {
                    self.take(byte);
                    if !self.escape()? {
                        bad = true;
                    }
                }
                _ if byte == quote => {
                    self.take(byte);
                    if quote == b'\'' && self.bytes.peek()? == Some(b'\'') {
                        self.take(b'\'');
                    } else {
                        self.state.quote = 0;
                        end = self.bytes.offset();
                        break;
                    }
                }
                _ => self.take(byte),
            }
            end = self.bytes.offset();
        }
        let kind = if bad {
            Kind::Invalid(Problem::BadEscape)
        } else if continued {
            Kind::QuotedLine
        } else {
            Kind::Quoted
        };
        Ok((kind, end))
    }

    /// The escape after a backslash; false when YAML does not know it. Never takes a line break:
    /// an escaped one carries the scalar into the next line.
    fn escape(&mut self) -> Result<bool, Failure> {
        let Some(byte) = self.bytes.peek()? else {
            return Ok(true);
        };
        let digits = match byte {
            b'\n' | b'\r' => return Ok(true),
            b'0' | b'a' | b'b' | b't' | b'\t' | b'n' | b'v' | b'f' | b'r' | b'e' | b' ' | b'"'
            | b'/' | b'\\' | b'N' | b'_' | b'L' | b'P' => 0,
            b'x' => 2,
            b'u' => 4,
            b'U' => 8,
            _ => {
                self.take(byte);
                return Ok(false);
            }
        };
        self.take(byte);
        for _ in 0..digits {
            match self.bytes.peek()? {
                Some(digit) if digit.is_ascii_hexdigit() => self.take(digit),
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    /// `|` or `>` with its indicators; the lines after it belong to the scalar while they are
    /// indented more than the node it is the value of: the key or `-` before it on this line,
    /// else a node as indented as this line's first.
    pub(super) fn block_header(&mut self, byte: u8) -> Result<(), Failure> {
        self.take(byte);
        let mut digit = None;
        while let Some(next) = self.bytes.peek()? {
            match next {
                b'1'..=b'9' => digit = Some(i64::from(next - b'0')),
                b'+' | b'-' => {}
                _ => break,
            }
            self.take(next);
        }
        let parent = self.node_col.map_or(i64::from(self.indent) - 1, i64::from);
        self.next_block = Some(Block {
            min: u32::try_from(parent + 1).unwrap_or(0),
            indent: digit.map(|digit| u32::try_from(parent + digit).unwrap_or(0)),
        });
        Ok(())
    }
}
