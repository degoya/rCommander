//! Recipe lines: make's prefixes `@`, `-` and `+`, then the shell command with the shell viewer's
//! colours as far as they need no grammar - keywords and builtins where a command starts, quoted
//! strings, the operators `| || && ; & > >> <`, brackets, comments - and make's references
//! everywhere, inside quotes too, since make expands them before the shell runs the line.
//! A quote ends with its line.

use super::{Continued, Emit, Lexer, is_blank, words};
use crate::types::TokenClass;

/// A byte that ends a shell word.
fn breaks_word(byte: u8) -> bool {
    is_blank(byte) || b"'\"$|&;<>(){}\\".contains(&byte)
}

impl<E: Emit> Lexer<'_, E> {
    /// The recipe from `from`: the prefixes, then the command.
    pub(crate) fn recipe(&mut self, from: usize) -> Continued {
        let start = self.skip_blanks(from);
        let end = self.run(start, |byte| matches!(byte, b'@' | b'-' | b'+'));
        self.push(start, end, TokenClass::Operator);
        self.shell(end);
        Continued::Recipe
    }

    /// A recipe line a backslash continued: the command goes on, without make's prefixes.
    pub(crate) fn recipe_goes_on(&mut self) -> Continued {
        let start = self.skip_blanks(0);
        self.shell(start);
        Continued::Recipe
    }

    /// A shell command line from `from`.
    fn shell(&mut self, from: usize) {
        let mut command = true;
        let mut at = from;
        while let Some(byte) = self.byte(at) {
            match byte {
                _ if is_blank(byte) => at += 1,
                b'#' if at == from || self.byte(at - 1).is_some_and(breaks_word) => {
                    self.comment(at);
                    return;
                }
                b'\'' | b'"' => {
                    at = self.shell_string(at, byte);
                    command = false;
                }
                b'$' => {
                    at = self.reference(at);
                    command = false;
                }
                b'\\' => at += 2,
                b'|' | b'&' | b';' | b'<' | b'>' => {
                    let end = self.run(at, |byte| b"|&;<>".contains(&byte));
                    self.push(at, end, TokenClass::Operator);
                    command = self.line[at..end]
                        .iter()
                        .any(|byte| matches!(byte, b'|' | b'&' | b';'));
                    at = end;
                }
                b'(' | b')' | b'{' | b'}' => {
                    self.push(at, at + 1, TokenClass::Punctuation);
                    command = matches!(byte, b'(' | b'{');
                    at += 1;
                }
                _ => {
                    let end = self.run(at, |byte| !breaks_word(byte));
                    let word = &self.line[at..end];
                    if command && words::is_shell_keyword(word) {
                        self.push(at, end, TokenClass::Keyword);
                        // `for x`, `case x`: a name or a word follows, no command.
                        command = !matches!(word, b"for" | b"case" | b"select" | b"function");
                    } else if command && words::is_shell_builtin(word) {
                        self.push(at, end, TokenClass::Function);
                        command = false;
                    } else if command && word.contains(&b'=') {
                        // `NAME=value command`: the command is still to come.
                    } else {
                        command = false;
                    }
                    at = end;
                }
            }
        }
    }

    /// The quoted string at `at`, opened with `quote`: its text as `string`, the references in
    /// it as theirs; in `"..."` a backslash escapes. The first byte after it.
    fn shell_string(&mut self, at: usize, quote: u8) -> usize {
        let mut piece = at;
        let mut pos = at + 1;
        while let Some(byte) = self.byte(pos) {
            if byte == quote {
                self.push(piece, pos + 1, TokenClass::String);
                return pos + 1;
            } else if byte == b'\\' && quote == b'"' {
                pos += 2;
            } else if byte == b'$' {
                self.push(piece, pos, TokenClass::String);
                pos = self.reference(pos);
                piece = pos;
            } else {
                pos += 1;
            }
        }
        let end = self.line.len();
        self.push(piece, end, TokenClass::String);
        end
    }
}
