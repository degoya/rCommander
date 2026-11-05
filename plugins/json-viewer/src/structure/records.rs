//! JSON Lines (`.jsonl`, `.ndjson`, RC-100-80): a value per line, each line checked for itself.
//!
//! The lines' values are the children of one list that starts with the text: its count and an
//! anchor every [`PAGE`] lines, as any long array has ([`Structure::records`]). A line that is
//! no value - broken, a value that goes on over the line's end, two values on one line - is a
//! [`Broken`] of its own, and the next line starts clean. At most [`MAX_BROKEN`] of them are
//! kept: past that the list ends at the next broken line, named as the error.

use super::{Builder, Entry, Expect, Frame, PAGE, ParseError, Structure};
use crate::lexer::Lexeme;

/// The most broken lines kept.
pub const MAX_BROKEN: usize = 1 << 14;

/// One line that is no value: where it starts, and its first error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Broken {
    pub(crate) offset: u64,
    pub(crate) error: ParseError,
}

/// The list of the lines being built.
#[derive(Debug)]
pub(crate) struct Records {
    list: Frame,
    /// The line of the value being read (1-based); 0 before the first.
    line: u64,
    /// Where that value starts.
    start: u64,
    /// The last lexeme of that line, for an error at its end.
    last: Option<Lexeme>,
    /// The line is broken: the rest of it is passed over.
    broken: bool,
}

impl Records {
    pub(crate) fn new(start: u64) -> Self {
        Self {
            list: Frame {
                start,
                object: false,
                count: 0,
                anchor_step: PAGE,
                anchors: Vec::new(),
            },
            line: 0,
            start,
            last: None,
            broken: false,
        }
    }
}

impl Structure {
    /// The broken line that starts at `offset`.
    pub(crate) fn broken_at(&self, offset: u64) -> Option<&Broken> {
        self.broken
            .binary_search_by_key(&offset, |broken| broken.offset)
            .ok()
            .and_then(|index| self.broken.get(index))
    }
}

impl Builder {
    pub(super) fn feed_record(&mut self, lexeme: &Lexeme) {
        if self.structure.error.is_some() {
            return;
        }
        let Some(records) = self.records.as_mut() else {
            return;
        };
        if lexeme.line != records.line {
            self.end_line("unexpected end of line");
            let Some(records) = self.records.as_mut() else {
                return;
            };
            records.line = lexeme.line;
            records.start = lexeme.start;
            records.broken = false;
            records.list.child(lexeme.start);
            self.stack.clear();
            self.expect = Expect::Value;
        }
        let Some(records) = self.records.as_mut() else {
            return;
        };
        if records.broken {
            return;
        }
        records.last = Some(*lexeme);
        if let Err(message) = self.step(lexeme) {
            self.break_line(lexeme, message);
        }
    }

    /// The line being read ends: broken when its value is not complete.
    fn end_line(&mut self, message: &'static str) {
        let Some(records) = self.records.as_ref() else {
            return;
        };
        if records.line == 0 || records.broken || self.complete() {
            return;
        }
        if let Some(last) = records.last {
            self.break_line(&last, message);
        }
    }

    /// The line being read is broken at `lexeme`.
    fn break_line(&mut self, lexeme: &Lexeme, message: &'static str) {
        let Some(records) = self.records.as_mut() else {
            return;
        };
        records.broken = true;
        let error = ParseError {
            offset: lexeme.start,
            line: lexeme.line,
            column: lexeme.column,
            message,
        };
        if self.structure.broken.len() >= MAX_BROKEN {
            // The list ends before this line.
            records.list.count = records.list.count.saturating_sub(1);
            self.structure.error = Some(ParseError {
                offset: records.start,
                message: "too many invalid lines",
                ..error
            });
            return;
        }
        self.structure.broken.push(Broken {
            offset: records.start,
            error,
        });
    }

    /// Ends the list at `eof`; the line still open is broken when it is not complete.
    pub(super) fn finish_records(&mut self, eof: u64) {
        self.end_line("unexpected end of file");
        self.stack.clear();
        let Some(records) = self.records.take() else {
            return;
        };
        let end = self
            .structure
            .error
            .as_ref()
            .map_or(eof, |error| error.offset);
        self.structure.records = Some(Entry {
            start: records.list.start,
            end,
            count: records.list.count,
            anchor_step: records.list.anchor_step,
            anchors: records.list.anchors.into_boxed_slice(),
        });
        self.structure.broken.shrink_to_fit();
    }
}

#[cfg(test)]
#[path = "records_tests.rs"]
mod tests;
