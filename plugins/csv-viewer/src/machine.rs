//! CSV as RFC 4180 has it, one byte at a time: a field in double quotes may hold the separator,
//! line breaks and `""` for a quote; a record ends with LF (a CR before it belongs to the line
//! break). The open pass, the tokens and the tree all read through this one machine, so they
//! agree on where every field and record starts.
//!
//! Lenient where editors are: a quote inside an unquoted field is text. Broken, and so `invalid`:
//! anything after a closing quote but the separator or the line break, and a quote still open at
//! the end of the file. A blank line is no record.

/// Where the machine is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Between records: the next byte that is not a line break starts one.
    #[default]
    RecordStart,
    /// After a separator: the next byte starts a field.
    FieldStart,
    Unquoted,
    Quoted,
    /// A quote inside a quoted field: the next byte tells an escaped quote from the closing one.
    QuoteSeen,
    /// A CR after the closing quote, waiting for its LF.
    Closed,
    /// Text after the closing quote: the field is broken until its end.
    Broken,
}

/// What one byte did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A line break (or CR) between records.
    Blank,
    /// Part of the field: `Some` the byte its value holds, `None` a quote of the syntax.
    Content(Option<u8>),
    /// The field ended at this byte, a separator (`record` false) or a line break (`record`
    /// true); `broken` when the field was.
    End { broken: bool, record: bool },
}

/// One byte's step: what it did, and whether it began a record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Step {
    pub(crate) began: bool,
    pub(crate) kind: Kind,
}

/// The machine over one separator.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Machine {
    pub(crate) mode: Mode,
    sep: u8,
}

impl Machine {
    pub(crate) fn new(sep: u8, mode: Mode) -> Self {
        Self { mode, sep }
    }

    /// Feeds one byte.
    pub(crate) fn feed(&mut self, byte: u8) -> Step {
        let began = self.mode == Mode::RecordStart && !matches!(byte, b'\n' | b'\r');
        if self.mode == Mode::RecordStart {
            if !began {
                return Step {
                    began,
                    kind: Kind::Blank,
                };
            }
            self.mode = Mode::FieldStart;
        }
        let kind = self.next(byte);
        Step { began, kind }
    }

    fn next(&mut self, byte: u8) -> Kind {
        let end = |mode: &mut Mode, broken: bool, record: bool| {
            *mode = if record {
                Mode::RecordStart
            } else {
                Mode::FieldStart
            };
            Kind::End { broken, record }
        };
        match self.mode {
            Mode::RecordStart | Mode::FieldStart | Mode::Unquoted | Mode::Broken => {
                let broken = self.mode == Mode::Broken;
                if byte == self.sep {
                    end(&mut self.mode, broken, false)
                } else if byte == b'\n' {
                    end(&mut self.mode, broken, true)
                } else if byte == b'"' && self.mode == Mode::FieldStart {
                    self.mode = Mode::Quoted;
                    Kind::Content(None)
                } else {
                    if self.mode == Mode::FieldStart {
                        self.mode = Mode::Unquoted;
                    }
                    Kind::Content(Some(byte))
                }
            }
            Mode::Quoted => {
                if byte == b'"' {
                    self.mode = Mode::QuoteSeen;
                    Kind::Content(None)
                } else {
                    Kind::Content(Some(byte))
                }
            }
            Mode::QuoteSeen => {
                if byte == b'"' {
                    self.mode = Mode::Quoted;
                    Kind::Content(Some(b'"'))
                } else if byte == self.sep {
                    end(&mut self.mode, false, false)
                } else if byte == b'\n' {
                    end(&mut self.mode, false, true)
                } else if byte == b'\r' {
                    self.mode = Mode::Closed;
                    Kind::Content(None)
                } else {
                    self.mode = Mode::Broken;
                    Kind::Content(Some(byte))
                }
            }
            Mode::Closed => {
                if byte == self.sep {
                    end(&mut self.mode, false, false)
                } else if byte == b'\n' {
                    end(&mut self.mode, false, true)
                } else {
                    self.mode = Mode::Broken;
                    Kind::Content(Some(byte))
                }
            }
        }
    }

    /// The end of the input: the last field's end, when a record is open; `broken` when the field
    /// was, or when its quote never closed.
    pub(crate) fn finish(&mut self) -> Option<Kind> {
        let mode = std::mem::take(&mut self.mode);
        (mode != Mode::RecordStart).then_some(Kind::End {
            broken: matches!(mode, Mode::Quoted | Mode::Broken),
            record: true,
        })
    }
}

#[cfg(test)]
#[path = "machine_tests.rs"]
mod tests;
