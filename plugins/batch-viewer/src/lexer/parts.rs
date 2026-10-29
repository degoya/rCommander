//! The pieces a command is made of: arguments with their strings and variables, operators and
//! redirections, and the byte classes cmd.exe splits a line by.

use super::{COMPARISONS, IfStep, Lexer, Mode};
use crate::types::TokenClass;

/// Separates arguments: whitespace and cmd.exe's other delimiters `,` `;` `=`.
pub(super) fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\t' | b'\r' | 0x0B | 0x0C | b',' | b';' | b'='
    )
}

/// Ends a command name, so `echo.`, `cd..`, `set/a` and `rem.` name their command.
pub(super) fn ends_command(byte: u8) -> bool {
    is_delimiter(byte)
        || matches!(
            byte,
            b'.' | b'/'
                | b':'
                | b'\\'
                | b'+'
                | b'['
                | b']'
                | b'('
                | b')'
                | b'"'
                | b'&'
                | b'|'
                | b'<'
                | b'>'
        )
}

/// Ends an argument outside quotes (a `)` only inside a block, which the caller knows).
fn ends_argument(byte: u8) -> bool {
    is_delimiter(byte) || matches!(byte, b'&' | b'|' | b'<' | b'>' | b'(')
}

/// The end of a label's name that starts at `from` (after its `:`).
pub(super) fn label_end(line: &[u8], from: usize) -> usize {
    from + line
        .get(from..)
        .unwrap_or_default()
        .iter()
        .take_while(|byte| !ends_argument(**byte) && !matches!(byte, b')' | b'^' | b'"'))
        .count()
}

/// A whole argument that is a number: decimal, negative or `0x` hexadecimal. `word` is lower
/// case.
pub(super) fn is_number(word: &[u8]) -> bool {
    if let Some(hex) = word.strip_prefix(b"0x") {
        return !hex.is_empty() && hex.iter().all(u8::is_ascii_hexdigit);
    }
    let digits = word.strip_prefix(b"-").unwrap_or(word);
    !digits.is_empty() && digits.iter().all(u8::is_ascii_digit)
}

/// The class of an `if` argument at `step` (`word` lower case, empty when not plain text) and
/// the step after it; `None` when the condition is complete and the command follows.
pub(super) fn if_step(step: IfStep, word: &[u8]) -> (Option<TokenClass>, Option<IfStep>) {
    match step {
        IfStep::Flags => match word {
            b"/i" | b"not" => (Some(TokenClass::Keyword), Some(IfStep::Flags)),
            b"exist" | b"defined" | b"errorlevel" | b"cmdextversion" => {
                (Some(TokenClass::Keyword), Some(IfStep::Operand))
            }
            _ => (None, Some(IfStep::Compare)),
        },
        IfStep::Compare if COMPARISONS.contains(&word) => {
            (Some(TokenClass::Operator), Some(IfStep::Operand))
        }
        IfStep::Compare | IfStep::Operand => (None, None),
    }
}

/// The end of the variable at `at`: `%name%` (with `:a=b` or `:~0,3`), `%1`, `%*`, `%~dp0`,
/// `%~$PATH:1`, `%%i`, `%%~nxi` and `!name!` (no blank inside, so `Done!` stays text). cmd.exe
/// expands `%` before it reads quotes, so a variable is found inside a string too.
pub(super) fn variable_end(line: &[u8], at: usize) -> Option<usize> {
    let rest = line.get(at + 1..)?;
    match *line.get(at)? {
        b'%' => match *rest.first()? {
            b'%' => {
                let name = rest.get(1..)?;
                if name.first() == Some(&b'~') {
                    let letters = name
                        .get(1..)?
                        .iter()
                        .take_while(|byte| byte.is_ascii_alphabetic())
                        .count();
                    return (letters > 0).then_some(at + 3 + letters);
                }
                name.first()
                    .filter(|byte| byte.is_ascii_alphanumeric())
                    .map(|_| at + 3)
            }
            b'~' => {
                let modifiers = rest
                    .get(1..)?
                    .iter()
                    .take_while(|byte| byte.is_ascii_alphabetic())
                    .count();
                let mut end = at + 2 + modifiers;
                if line.get(end) == Some(&b'$') {
                    let colon = line
                        .get(end..)?
                        .iter()
                        .take_while(|byte| !is_delimiter(**byte))
                        .position(|byte| *byte == b':')?;
                    end += colon + 1;
                }
                line.get(end)
                    .filter(|byte| byte.is_ascii_digit())
                    .map(|_| end + 1)
            }
            b'0'..=b'9' | b'*' => Some(at + 2),
            _ => rest
                .iter()
                .position(|byte| *byte == b'%')
                .map(|close| at + close + 2),
        },
        b'!' => {
            let close = rest
                .iter()
                .position(|byte| matches!(byte, b'!' | b' ' | b'\t'))?;
            (close > 0 && rest.get(close) == Some(&b'!')).then_some(at + close + 2)
        }
        _ => None,
    }
}

/// The bytes a `%` or `!` that starts no variable takes: two for the escaped `%%`.
pub(super) fn escape_width(line: &[u8], at: usize) -> usize {
    if line.get(at..at + 2) == Some(b"%%") {
        2
    } else {
        1
    }
}

/// `nul` as a whole word at `at`.
fn is_nul(line: &[u8], at: usize) -> bool {
    line.get(at..at + 3)
        .is_some_and(|word| word.eq_ignore_ascii_case(b"nul"))
        && line
            .get(at + 3)
            .is_none_or(|byte| ends_argument(*byte) || *byte == b')')
}

impl Lexer<'_> {
    /// One argument from `pos`: its strings and variables as tokens. Returns its end and whether
    /// it is plain text - no string, variable or `^` in it.
    pub(super) fn item(&mut self) -> (usize, bool) {
        let mut plain = true;
        while let Some(byte) = self.byte(self.pos) {
            if ends_argument(byte) || (byte == b')' && self.depth > 0) {
                break;
            }
            match byte {
                b'"' => {
                    plain = false;
                    self.string();
                }
                b'%' | b'!' => {
                    if self.variable_or_skip() {
                        plain = false;
                    }
                }
                b'^' => {
                    plain = false;
                    self.caret();
                }
                _ => self.pos += 1,
            }
        }
        (self.pos, plain)
    }

    /// A string from the `"` at `pos` to the next `"` or the line's end, split around the
    /// variables in it.
    fn string(&mut self) {
        let mut segment = self.pos;
        self.pos += 1;
        while let Some(byte) = self.byte(self.pos) {
            match byte {
                b'"' => {
                    self.pos += 1;
                    break;
                }
                b'%' | b'!' => match variable_end(self.line, self.pos) {
                    Some(end) => {
                        self.push(segment, self.pos, TokenClass::String);
                        self.push(self.pos, end, TokenClass::Key);
                        self.pos = end;
                        segment = end;
                    }
                    None => self.pos += escape_width(self.line, self.pos),
                },
                _ => self.pos += 1,
            }
        }
        self.push(segment, self.pos.min(self.line.len()), TokenClass::String);
    }

    /// An operator at `pos`: `&`, `&&`, `|`, `||` (a command follows) or a redirection. False
    /// when there is none.
    pub(super) fn operator(&mut self) -> bool {
        let at = self.pos;
        match self.byte(at) {
            Some(byte @ (b'&' | b'|')) => {
                let end = if self.byte(at + 1) == Some(byte) {
                    at + 2
                } else {
                    at + 1
                };
                self.push(at, end, TokenClass::Operator);
                self.pos = end;
                self.mode = Mode::Command;
                true
            }
            Some(b'<' | b'>') => {
                self.redirect(at, at);
                true
            }
            Some(b'0'..=b'9') if matches!(self.byte(at + 1), Some(b'<' | b'>')) => {
                self.redirect(at, at + 1);
                true
            }
            _ => false,
        }
    }

    /// A redirection from `start` (a handle digit, or the arrow at `arrow`): `>`, `>>`, `<`,
    /// `2>&1`, `>nul`. Its target is read here, so it is never taken for an argument.
    fn redirect(&mut self, start: usize, arrow: usize) {
        let mut end = arrow + 1;
        if self.byte(arrow) == Some(b'>') && self.byte(end) == Some(b'>') {
            end += 1;
        }
        if self.byte(end) == Some(b'&') && self.byte(end + 1).is_some_and(|b| b.is_ascii_digit()) {
            self.push(start, end + 2, TokenClass::Operator);
            self.pos = end + 2;
            return;
        }
        let target = end
            + self
                .line
                .get(end..)
                .unwrap_or_default()
                .iter()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
        if is_nul(self.line, target) {
            if target == end {
                self.push(start, target + 3, TokenClass::Operator);
            } else {
                self.push(start, end, TokenClass::Operator);
                self.push(target, target + 3, TokenClass::Operator);
            }
            self.pos = target + 3;
            return;
        }
        self.push(start, end, TokenClass::Operator);
        self.pos = target;
        self.item();
    }
}
