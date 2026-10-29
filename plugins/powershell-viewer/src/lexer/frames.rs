//! The open constructs a line starts inside of, as a stack the line index keeps at every
//! checkpoint: the shell viewer's frames, with PowerShell's kinds.

/// A construct that goes on until its closer, possibly over several lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Frame {
    /// `(`, `$(` or `@(` ... `)` opened where an expression stands: code, a value after it.
    Paren = 1,
    /// The same opened among a command's arguments: the arguments go on after it.
    ParenArgs = 2,
    /// `"` ... `"`: variables and `$( )` expand.
    DQuote = 3,
    /// `'` ... `'`: nothing expands.
    SQuote = 4,
    /// `@"` ... `"@` over lines: variables and `$( )` expand.
    HereDQ = 5,
    /// `@'` ... `'@` over lines.
    HereSQ = 6,
    /// `<#` ... `#>`
    Comment = 7,
}

impl Frame {
    fn from_bits(bits: u64) -> Self {
        match bits {
            2 => Self::ParenArgs,
            3 => Self::DQuote,
            4 => Self::SQuote,
            5 => Self::HereDQ,
            6 => Self::HereSQ,
            7 => Self::Comment,
            _ => Self::Paren,
        }
    }

    /// A string: its opening shows as `invalid` when the file leaves it open.
    pub(crate) fn is_string(self) -> bool {
        matches!(
            self,
            Self::DQuote | Self::SQuote | Self::HereDQ | Self::HereSQ
        )
    }

    /// Text rather than code: a line inside it goes on where the line before stopped.
    pub(crate) fn is_text(self) -> bool {
        self.is_string() || self == Self::Comment
    }
}

/// The frames a stack keeps by kind, four bits each; deeper ones are only counted.
pub(crate) const SLOTS: u16 = 16;

/// The open frames, innermost on top. Past [`SLOTS`] a frame's kind is not kept and it is taken
/// for a `(` - no script nests that deep, and the state stays a few bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Frames {
    bits: u64,
    depth: u16,
}

impl Frames {
    pub(crate) fn top(self) -> Option<Frame> {
        match self.depth {
            0 => None,
            depth if depth > SLOTS => Some(Frame::Paren),
            depth => Some(Frame::from_bits((self.bits >> (4 * (depth - 1))) & 0xF)),
        }
    }

    /// Pushes `frame`; false when it is only counted, its kind lost.
    pub(crate) fn push(&mut self, frame: Frame) -> bool {
        let kept = self.depth < SLOTS;
        if kept {
            self.bits |= (frame as u64) << (4 * self.depth);
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
            self.bits &= !(0xF_u64 << (4 * self.depth));
        }
    }
}
