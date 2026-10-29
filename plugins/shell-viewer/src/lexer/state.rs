//! What a line starts inside of: the [`LineState`] the line index keeps at every checkpoint, so
//! the lexer starts at any of them as if it had read everything before.

/// A construct that goes on until its closer, possibly over several lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Frame {
    /// `$(` ... `)`: commands.
    Subst = 1,
    /// `(` ... `)`, `<(` ... `)`, `x=(` ... `)`: a subshell, a process substitution, an array.
    Group = 2,
    /// `"` ... `"`, `$"` ... `"`
    DQuote = 3,
    /// `${` ... `}`
    Param = 4,
    /// `$((` or `((` ... `))`
    Arith = 5,
    /// `(` ... `)` inside arithmetic.
    ArithParen = 6,
    /// `` ` `` ... `` ` ``: commands.
    Backtick = 7,
    /// `case ... in`, at a pattern: its `)` starts the commands.
    CasePattern = 8,
    /// The commands after a pattern, until `;;`, `;&`, `;;&` or `esac`.
    CaseBody = 9,
}

impl Frame {
    fn from_bits(bits: u64) -> Self {
        match bits {
            2 => Self::Group,
            3 => Self::DQuote,
            4 => Self::Param,
            5 => Self::Arith,
            6 => Self::ArithParen,
            7 => Self::Backtick,
            8 => Self::CasePattern,
            9 => Self::CaseBody,
            _ => Self::Subst,
        }
    }
}

/// The frames a stack keeps by kind, four bits each; deeper ones are only counted.
const SLOTS: u16 = 16;

/// The open frames, innermost on top. Past [`SLOTS`] a frame's kind is not kept and it is taken
/// for a command substitution - no script nests that deep, and the state stays a few bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Frames {
    bits: u64,
    depth: u16,
}

impl Frames {
    pub(crate) fn top(self) -> Option<Frame> {
        match self.depth {
            0 => None,
            depth if depth > SLOTS => Some(Frame::Subst),
            depth => Some(Frame::from_bits((self.bits >> (4 * (depth - 1))) & 0xF)),
        }
    }

    pub(crate) fn push(&mut self, frame: Frame) {
        if self.depth < SLOTS {
            self.bits |= (frame as u64) << (4 * self.depth);
        }
        self.depth = self.depth.saturating_add(1);
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

    pub(crate) fn replace_top(&mut self, frame: Frame) {
        self.pop();
        self.push(frame);
    }
}

/// Where the next word stands in its command: what makes `if` a keyword, `echo` a builtin and
/// `x=` an assignment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Pos {
    /// A command's first word: a keyword, a builtin, an assignment, a function's name.
    #[default]
    Command,
    /// An argument.
    Args,
    /// The arguments of `export`, `local`, `declare`, `readonly`, `typeset`: assignments.
    Decl,
    /// After `function`: the function's name.
    FuncName,
    /// After a function's name: its `(`.
    FuncOpen,
    /// After a function's `(`: its `)`.
    FuncClose,
    /// After `for` or `select`: the variable.
    ForVar,
    /// After `for x`: `in` or `do`.
    ForIn,
    /// After `case`: the word.
    CaseWord,
    /// After `case x`: `in`.
    CaseIn,
}

/// A single-quoted string still open: nothing in it is special, in `$'...'` but `\`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Quote {
    #[default]
    None,
    /// `'...'`
    Single,
    /// `$'...'`
    Dollar,
}

/// A heredoc whose lines are being read: its marker by length and hash, so the state stays
/// `Copy` and small whatever the marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Heredoc {
    pub(crate) hash: u64,
    pub(crate) len: u16,
    /// `<<-`: leading tabs of the lines, the marker's too, are not part of them.
    pub(crate) strip_tabs: bool,
    /// The marker was quoted (`<<'EOF'`, `<<"EOF"`, `<<\EOF`): nothing in the text expands.
    pub(crate) literal: bool,
}

/// What a line starts inside of: all a lexer needs to start there.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    pub(crate) frames: Frames,
    pub(crate) quote: Quote,
    pub(crate) heredoc: Option<Heredoc>,
    /// Where the line's first word stands: a command's start, unless a `\` at the end of the
    /// line before continued a command.
    pub(crate) pos: Pos,
}

/// FNV-1a over a heredoc marker's bytes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Fnv(u64);

impl Fnv {
    /// The hash of no bytes.
    pub(crate) fn start() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    pub(crate) fn add(&mut self, byte: u8) {
        self.0 = (self.0 ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
    }

    pub(crate) fn of(bytes: &[u8]) -> u64 {
        let mut hash = Self::start();
        bytes.iter().for_each(|byte| hash.add(*byte));
        hash.0
    }

    pub(crate) fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
