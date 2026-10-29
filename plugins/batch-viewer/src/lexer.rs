//! The batch lexer: one line at a time, in the state the line before left, as cmd.exe reads a
//! script - case-insensitive, a command name where a command starts, its arguments after it.
//!
//! Only two things reach from one line into the next ([`LineState`]): a `^` at the end of a line,
//! which goes on with the next line in the same mode, and the depth of open `( ... )` blocks,
//! inside which a `)` closes the block even in `echo`'s text. cmd.exe knows no string over
//! several lines. A comment (`rem`, `::`) ends with its line.
//!
//! The lexer decides on ASCII bytes alone; any other byte is part of a word and never starts or
//! ends a token in the middle of a character. So it reaches the same state on the file's bytes
//! (`open`'s index) as on their UTF-8 text (`tokens`), and every token sits on character
//! boundaries.

mod parts;

use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;

/// What a line starts inside of.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct LineState {
    /// Open `(` blocks.
    pub(crate) depth: u16,
    /// The line before ended in a `^`: this one goes on in that mode.
    pub(crate) carry: Option<Mode>,
}

/// Where the lexer is within a command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Where a command name is read: a line's start, after `(`, `&`, `|`, `do`, `else`.
    Command,
    /// A command's arguments.
    Args(Context),
    /// `echo`'s text: uncoloured but for variables, operators and a block's `)`.
    Echo { quoted: bool },
}

/// The command whose arguments are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Context {
    Plain,
    /// After `goto` (`bare`: a label without its `:`) or `call` (a label only with its `:`).
    Label {
        bare: bool,
    },
    /// After `for`: `in`, `do` and the switches.
    For,
    /// After `if`, at a step of its condition.
    If(IfStep),
}

/// The steps of `if [/i] [not] <condition> <command>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum IfStep {
    /// `/i`, `not`, a keyword condition or the first operand.
    Flags,
    /// The comparison after the first operand: `==`, `equ`, `neq`, ...
    Compare,
    /// The last operand; the command follows.
    Operand,
}

/// The command names coloured as keywords (`for`'s `in` and `do` and `if`'s words come in their
/// context).
const COMMANDS: &[&[u8]] = &[
    b"if",
    b"else",
    b"for",
    b"in",
    b"do",
    b"goto",
    b"call",
    b"set",
    b"setlocal",
    b"endlocal",
    b"echo",
    b"exit",
    b"shift",
    b"pushd",
    b"popd",
    b"cd",
    b"chdir",
    b"start",
    b"title",
    b"color",
    b"cls",
    b"pause",
    b"choice",
    b"rem",
];

/// The comparisons of `if`, as words.
const COMPARISONS: &[&[u8]] = &[b"equ", b"neq", b"lss", b"leq", b"gtr", b"geq"];

/// The switches coloured as keywords.
const SWITCHES: &[&[u8]] = &[b"/a", b"/p", b"/i", b"/f", b"/d", b"/r", b"/l"];

/// Lexes `line` (without its line break and a `\r` before it) from `state`; the tokens go to
/// `out` when given, at most [`MAX_TOKENS_PER_LINE`]. Returns the state the next line starts in.
pub(crate) fn lex_line(line: &[u8], state: LineState, out: Option<&mut Vec<Token>>) -> LineState {
    let mut lexer = Lexer {
        line,
        pos: 0,
        depth: state.depth,
        mode: state.carry.unwrap_or(Mode::Command),
        continued: false,
        out,
    };
    if state.carry.is_some()
        && matches!(
            line.first(),
            Some(b'&' | b'|' | b'<' | b'>' | b'(' | b')' | b'"')
        )
    {
        // The `^` that ended the line before escapes the first character of this one.
        lexer.pos = 1;
        if lexer.mode == Mode::Command {
            lexer.mode = Mode::Args(Context::Plain);
        }
    }
    while lexer.pos < line.len() {
        match lexer.mode {
            Mode::Command => lexer.command(),
            Mode::Args(context) => lexer.args(context),
            Mode::Echo { quoted } => lexer.echo(quoted),
        }
    }
    LineState {
        depth: lexer.depth,
        carry: lexer.continued.then_some(lexer.mode),
    }
}

struct Lexer<'a> {
    line: &'a [u8],
    pos: usize,
    depth: u16,
    mode: Mode,
    /// The line ended in a `^`.
    continued: bool,
    out: Option<&'a mut Vec<Token>>,
}

impl Lexer<'_> {
    fn byte(&self, at: usize) -> Option<u8> {
        self.line.get(at).copied()
    }

    /// Adds the token `start..end` when tokens are kept and the line has room.
    fn push(&mut self, start: usize, end: usize, class: TokenClass) {
        let Some(out) = self.out.as_deref_mut() else {
            return;
        };
        let (Ok(at), Ok(len)) = (
            u32::try_from(start),
            u32::try_from(end.saturating_sub(start)),
        ) else {
            return;
        };
        if len > 0 && out.len() < MAX_TOKENS_PER_LINE {
            out.push(Token {
                start: at,
                len,
                class,
            });
        }
    }

    /// One step where a command name is read.
    fn command(&mut self) {
        let at = self.pos;
        let Some(byte) = self.byte(at) else {
            return;
        };
        match byte {
            _ if parts::is_delimiter(byte) => self.pos += 1,
            b'@' => {
                self.push(at, at + 1, TokenClass::Punctuation);
                self.pos += 1;
            }
            b'(' => self.open_block(),
            b')' => {
                self.close_block();
                self.pos += 1;
            }
            b':' if self.byte(at + 1) == Some(b':') => self.comment(),
            b':' => {
                // A label; cmd.exe reads nothing after its name.
                let end = parts::label_end(self.line, at + 1);
                self.push(at, end, TokenClass::Function);
                self.pos = self.line.len();
            }
            b'^' => {
                self.caret();
                if !self.continued {
                    self.mode = Mode::Args(Context::Plain);
                }
            }
            _ if self.operator() => {}
            _ => {
                if !self.keyword() {
                    self.item();
                    self.mode = Mode::Args(Context::Plain);
                }
            }
        }
    }

    /// A command name that is a keyword: coloured, and the mode its arguments are read in.
    fn keyword(&mut self) -> bool {
        let at = self.pos;
        let end = at
            + self.line[at..]
                .iter()
                .take_while(|byte| byte.is_ascii_alphabetic())
                .count();
        if end == at || !self.byte(end).is_none_or(parts::ends_command) {
            return false;
        }
        let word = self.line[at..end].to_ascii_lowercase();
        if !COMMANDS.contains(&word.as_slice()) {
            return false;
        }
        if word == b"rem" {
            self.comment();
            return true;
        }
        self.push(at, end, TokenClass::Keyword);
        self.pos = end;
        self.mode = match word.as_slice() {
            b"echo" => Mode::Echo { quoted: false },
            b"goto" => Mode::Args(Context::Label { bare: true }),
            b"call" => Mode::Args(Context::Label { bare: false }),
            b"for" | b"in" => Mode::Args(Context::For),
            b"if" => Mode::Args(Context::If(IfStep::Flags)),
            b"else" | b"do" => Mode::Command,
            _ => Mode::Args(Context::Plain),
        };
        true
    }

    /// A comment from here to the line's end.
    fn comment(&mut self) {
        self.push(self.pos, self.line.len(), TokenClass::Comment);
        self.pos = self.line.len();
    }

    /// One step through a command's arguments.
    fn args(&mut self, context: Context) {
        let at = self.pos;
        let Some(byte) = self.byte(at) else {
            return;
        };
        let comparing = matches!(context, Context::If(IfStep::Flags | IfStep::Compare));
        match byte {
            b'=' if comparing && self.byte(at + 1) == Some(b'=') => {
                self.push(at, at + 2, TokenClass::Operator);
                self.pos += 2;
                self.mode = Mode::Args(Context::If(IfStep::Operand));
            }
            _ if parts::is_delimiter(byte) => self.pos += 1,
            b'^' => self.caret(),
            b'(' => {
                self.open_block();
                if context == Context::For {
                    self.mode = Mode::Args(Context::For);
                }
            }
            b')' if self.depth > 0 => {
                self.close_block();
                self.pos += 1;
                if context != Context::For {
                    self.mode = Mode::Command;
                }
            }
            b':' if context == (Context::Label { bare: false }) => {
                let end = parts::label_end(self.line, at + 1);
                self.push(at, end, TokenClass::Function);
                self.pos = end;
                self.mode = Mode::Args(Context::Plain);
            }
            // `call` with anything but a label calls a command.
            _ if context == (Context::Label { bare: false }) => self.mode = Mode::Command,
            _ if self.operator() => {}
            _ => {
                let (end, plain) = self.item();
                self.argument(context, at, end, plain);
            }
        }
    }

    /// Colours a whole argument `start..end` by its context and moves the context on. `plain`:
    /// no string, variable or `^` in it, so its text is what it says.
    fn argument(&mut self, context: Context, start: usize, end: usize, plain: bool) {
        let word = if plain {
            self.line[start..end].to_ascii_lowercase()
        } else {
            Vec::new()
        };
        let word = word.as_slice();
        let mut class = parts::is_number(word).then_some(TokenClass::Number);
        if SWITCHES.contains(&word) {
            class = Some(TokenClass::Keyword);
        }
        let next = match context {
            Context::Plain => context,
            Context::Label { .. } => {
                if plain {
                    class = Some(TokenClass::Function);
                }
                Context::Plain
            }
            Context::For => match word {
                b"in" => {
                    class = Some(TokenClass::Keyword);
                    context
                }
                b"do" => {
                    self.push(start, end, TokenClass::Keyword);
                    self.mode = Mode::Command;
                    return;
                }
                _ => context,
            },
            Context::If(step) => {
                let (word_class, next) = parts::if_step(step, word);
                if word_class.is_some() {
                    class = word_class;
                }
                match next {
                    Some(step) => Context::If(step),
                    None => {
                        if let Some(class) = class {
                            self.push(start, end, class);
                        }
                        self.mode = Mode::Command;
                        return;
                    }
                }
            }
        };
        if let Some(class) = class {
            self.push(start, end, class);
        }
        self.mode = Mode::Args(next);
    }

    /// One step through `echo`'s text.
    fn echo(&mut self, quoted: bool) {
        let at = self.pos;
        let Some(byte) = self.byte(at) else {
            return;
        };
        // `2>` redirects only as a word of its own: `echo a2>b` writes `a2`.
        let word_start = at == 0 || self.byte(at - 1).is_some_and(parts::is_delimiter);
        match byte {
            b'%' | b'!' => {
                self.variable_or_skip();
            }
            b'"' => {
                self.mode = Mode::Echo { quoted: !quoted };
                self.pos += 1;
            }
            _ if quoted => self.pos += 1,
            b'^' => self.caret(),
            b')' if self.depth > 0 => {
                self.close_block();
                self.pos += 1;
                self.mode = Mode::Command;
            }
            b'&' | b'|' | b'<' | b'>' => {
                self.operator();
            }
            b'0'..=b'9' if word_start && self.operator() => {}
            _ => self.pos += 1,
        }
    }

    /// `(`: a block opens, and a command follows.
    fn open_block(&mut self) {
        self.push(self.pos, self.pos + 1, TokenClass::Punctuation);
        self.pos += 1;
        self.depth = self.depth.saturating_add(1);
        self.mode = Mode::Command;
    }

    /// `)` at `pos`: the innermost block closes.
    fn close_block(&mut self) {
        self.push(self.pos, self.pos + 1, TokenClass::Punctuation);
        self.depth = self.depth.saturating_sub(1);
    }

    /// `^`: escapes the next character, or goes on with the next line at the line's end.
    fn caret(&mut self) {
        if self.pos + 1 >= self.line.len() {
            self.continued = true;
            self.pos = self.line.len();
        } else {
            self.pos += 2;
        }
    }

    /// A variable from `pos` as a `key`, else past the `%` or `!` - or the escaped `%%`.
    /// Returns whether there was one.
    fn variable_or_skip(&mut self) -> bool {
        match parts::variable_end(self.line, self.pos) {
            Some(end) => {
                self.push(self.pos, end, TokenClass::Key);
                self.pos = end;
                true
            }
            None => {
                self.pos += parts::escape_width(self.line, self.pos);
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "lexer_tests.rs"]
mod tests;
