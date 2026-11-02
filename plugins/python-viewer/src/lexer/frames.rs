//! The open constructs a line starts inside of, as a stack the line index keeps at every
//! checkpoint: the PowerShell viewer's frames, with Python's kinds - strings with their quote and
//! prefix, an f-string's replacement field and its format spec, and code brackets, counted.

/// How a string was opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Quote {
    /// `"` rather than `'`.
    pub(crate) double: bool,
    /// `"""` or `'''`: the string goes on over lines.
    pub(crate) triple: bool,
    /// `r`: `\N{...}` is no escape, so its braces open a field in a raw f-string.
    pub(crate) raw: bool,
    /// `f` or `t`: `{ }` hold expressions, `{{` and `}}` are braces.
    pub(crate) fields: bool,
}

impl Quote {
    /// The quote character.
    pub(crate) fn byte(self) -> u8 {
        if self.double { b'"' } else { b'\'' }
    }

    fn bits(self) -> u8 {
        STRING
            | u8::from(self.double)
            | (u8::from(self.triple) << 1)
            | (u8::from(self.raw) << 2)
            | (u8::from(self.fields) << 3)
    }

    fn from_bits(bits: u8) -> Self {
        Self {
            double: bits & 1 != 0,
            triple: bits & 2 != 0,
            raw: bits & 4 != 0,
            fields: bits & 8 != 0,
        }
    }
}

/// A construct that goes on until its closer, possibly over several lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Frame {
    /// `(`, `[` or `{` of code, this many nested: any closer closes one. Their kind does not
    /// matter for colouring, so one slot counts up to [`MAX_COUNT`] of them.
    Brackets(u8),
    /// A string's text.
    Str(Quote),
    /// An f-string's replacement field `{ ... }`: code up to its `}`.
    Field,
    /// The format spec after a field's `:`: text, with fields of its own, up to the `}`.
    Spec,
}

const STRING: u8 = 0x80;
const FIELD: u8 = 0x40;
const SPEC: u8 = 0x41;
/// The most brackets one slot counts.
const MAX_COUNT: u8 = 0x3F;

impl Frame {
    fn bits(self) -> u8 {
        match self {
            Self::Brackets(count) => count.clamp(1, MAX_COUNT),
            Self::Str(quote) => quote.bits(),
            Self::Field => FIELD,
            Self::Spec => SPEC,
        }
    }

    fn from_bits(bits: u8) -> Self {
        match bits {
            _ if bits & STRING != 0 => Self::Str(Quote::from_bits(bits)),
            FIELD => Self::Field,
            SPEC => Self::Spec,
            count => Self::Brackets(count.max(1)),
        }
    }

    /// Text rather than code: a line inside it goes on where the line before stopped.
    #[cfg(test)]
    pub(crate) fn is_text(self) -> bool {
        matches!(self, Self::Str(_) | Self::Spec)
    }
}

/// The frames a stack keeps by kind, eight bits each; deeper ones are only counted.
pub(crate) const SLOTS: u16 = 8;

/// The open frames, innermost on top. Past [`SLOTS`] a frame's kind is not kept and it is taken
/// for a bracket; strings and fields open there are read on their line alone (the lexer asks
/// [`Frames::full`]). Brackets share a slot, so only strings inside fields inside strings take
/// slots - no script nests eight of those, and the state stays a few bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Frames {
    bits: u64,
    depth: u16,
}

impl Frames {
    pub(crate) fn depth(self) -> u16 {
        self.depth
    }

    /// No slot is left for a frame of its own kind.
    pub(crate) fn full(self) -> bool {
        self.depth >= SLOTS
    }

    pub(crate) fn top(self) -> Option<Frame> {
        match self.depth {
            0 => None,
            depth if depth > SLOTS => Some(Frame::Brackets(1)),
            depth => Some(Frame::from_bits(
                ((self.bits >> (8 * (depth - 1))) & 0xFF) as u8,
            )),
        }
    }

    /// Pushes `frame`; false when it is only counted, its kind lost.
    pub(crate) fn push(&mut self, frame: Frame) -> bool {
        let kept = self.depth < SLOTS;
        if kept {
            self.bits |= u64::from(frame.bits()) << (8 * self.depth);
        }
        self.depth = self.depth.saturating_add(1);
        kept
    }

    pub(crate) fn pop(&mut self) {
        if self.depth == 0 {
            return;
        }
        self.depth -= 1;
        if self.depth < SLOTS {
            self.bits &= !(0xFF_u64 << (8 * self.depth));
        }
    }

    /// Replaces the top frame's kind, when it is kept.
    fn set_top(&mut self, frame: Frame) {
        if self.depth == 0 || self.depth > SLOTS {
            return;
        }
        let shift = 8 * (self.depth - 1);
        self.bits = (self.bits & !(0xFF_u64 << shift)) | (u64::from(frame.bits()) << shift);
    }

    /// `(`, `[` or `{` of code.
    pub(crate) fn open_bracket(&mut self) {
        match self.top() {
            Some(Frame::Brackets(count)) if self.depth <= SLOTS && count < MAX_COUNT => {
                self.set_top(Frame::Brackets(count + 1));
            }
            _ => {
                self.push(Frame::Brackets(1));
            }
        }
    }

    /// `)`, `]` or `}` of code: closes one bracket, if one is open on top.
    pub(crate) fn close_bracket(&mut self) {
        match self.top() {
            Some(Frame::Brackets(count)) if count > 1 && self.depth <= SLOTS => {
                self.set_top(Frame::Brackets(count - 1));
            }
            Some(Frame::Brackets(_)) => self.pop(),
            _ => {}
        }
    }

    /// A field's `:`: the rest of the field is its format spec.
    pub(crate) fn field_to_spec(&mut self) {
        if self.top() == Some(Frame::Field) {
            self.set_top(Frame::Spec);
        }
    }
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
