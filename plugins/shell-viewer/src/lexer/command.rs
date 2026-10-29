//! Commands: words, keywords, builtins, assignments, operators, redirections, brackets and
//! comments - everything outside a quote, `${` or `$((`.

use super::state::{Frame, Pos, Quote};
use super::words::{self, WORD_BYTES};
use super::{Ctx, Lexeme, Lexer, Role};
use crate::types::{Failure, Source, TokenClass};

/// How far a word is looked at before it is taken: its text, an assignment's `=`, a function's
/// `()` after it.
const LOOK: usize = 256;

impl<S: Source> Lexer<'_, S> {
    /// The lexeme at `byte` among commands.
    pub(super) fn in_command(&mut self, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        if let Some(strip) = self.marker.take() {
            if !words::is_meta(byte) {
                return self.heredoc_marker(strip, start);
            }
            if matches!(byte, b' ' | b'\t') {
                self.marker = Some(strip);
            }
        }
        match byte {
            b' ' | b'\t' | b'\r' => self.blanks(start),
            b'\\' if self.continues()? => {
                self.take()?;
                self.continued = true;
                Ok(self.lexeme(None, start))
            }
            b'#' if !self.in_word => self.comment(start),
            b';' | b'&' | b'|' => self.control(byte, start),
            b'<' | b'>' => self.redirection(start),
            b'(' => self.open_paren(start),
            b')' => self.close_paren(start),
            b'`' => self.backtick(start, Ctx::Command),
            b'"' => {
                self.start_word();
                self.open_dquote(start, 1)
            }
            b'\'' => {
                self.start_word();
                self.open_single(Quote::Single, start, 1)
            }
            b'$' => self.dollar(start, Ctx::Command),
            b'0'..=b'9' if !self.in_word => {
                let fd = words::fd_prefix(self.reader.look(LOOK)?);
                match fd {
                    Some(digits) => {
                        self.take_n(digits)?;
                        self.redirection(start)
                    }
                    None => self.word(start),
                }
            }
            _ => self.word(start),
        }
    }

    /// Spaces and tabs, and a carriage return not before a line break: no colour, the end of a
    /// word.
    pub(super) fn blanks(&mut self, start: u32) -> Result<Lexeme, Failure> {
        while !self.full(start) {
            match self.peek()? {
                Some(b' ' | b'\t') => {}
                Some(b'\r') if self.peek_at(1)? != Some(b'\n') => {}
                _ => break,
            }
            self.take()?;
        }
        self.end_word();
        Ok(self.lexeme(None, start))
    }

    /// A comment, or a shebang, to the end of its line.
    pub(super) fn comment(&mut self, start: u32) -> Result<Lexeme, Failure> {
        while !self.full(start) && !self.at_line_end()? {
            self.take()?;
        }
        self.in_comment = !self.at_line_end()?;
        Ok(self.lexeme(Some(TokenClass::Comment), start))
    }

    /// `;`, `;;`, `;&`, `;;&`, `&`, `&&`, `&>`, `&>>`, `|`, `||`, `|&`.
    fn control(&mut self, byte: u8, start: u32) -> Result<Lexeme, Failure> {
        self.end_word();
        self.take()?;
        let next = self.peek()?;
        let mut command = true;
        match (byte, next) {
            (b';', Some(b';')) => {
                self.take()?;
                if self.peek()? == Some(b'&') {
                    self.take()?;
                }
                self.case_item_end();
            }
            (b';', Some(b'&')) => {
                self.take()?;
                self.case_item_end();
            }
            (b'&', Some(b'>')) => {
                self.take()?;
                if self.peek()? == Some(b'>') {
                    self.take()?;
                }
                self.redirect = true;
                command = false;
            }
            (b'&', Some(b'&')) | (b'|', Some(b'|' | b'&')) => {
                self.take()?;
            }
            _ => {}
        }
        // A `|` between case patterns leaves the pattern.
        if command && self.state.frames.top() != Some(Frame::CasePattern) {
            self.state.pos = Pos::Command;
        }
        Ok(self.lexeme(Some(TokenClass::Operator), start))
    }

    /// `;;`, `;&` or `;;&` ends a case item: the next pattern follows.
    fn case_item_end(&mut self) {
        if self.state.frames.top() == Some(Frame::CaseBody) {
            self.state.frames.replace_top(Frame::CasePattern);
        }
    }

    /// A redirection, from an fd number taken before it: `<`, `>`, `>>`, `>|`, `<>`, `<&n`,
    /// `>&n`, `<<<`, a heredoc's `<<` or `<<-`, a process substitution's `<(` or `>(`.
    fn redirection(&mut self, start: u32) -> Result<Lexeme, Failure> {
        self.end_word();
        let first = self.take()?;
        match (first, self.peek()?) {
            (Some(b'<'), Some(b'<')) => {
                self.take()?;
                match self.peek()? {
                    Some(b'<') => {
                        self.take()?;
                        self.redirect = true;
                    }
                    Some(b'-') => {
                        self.take()?;
                        self.marker = Some(true);
                    }
                    _ => self.marker = Some(false),
                }
            }
            (_, Some(b'(')) => {
                self.take()?;
                self.state.frames.push(Frame::Group);
                self.state.pos = Pos::Command;
            }
            (_, Some(b'&')) => {
                self.take()?;
                let mut target = false;
                while self.peek()?.is_some_and(|byte| byte.is_ascii_digit()) {
                    self.take()?;
                    target = true;
                }
                if !target && self.peek()? == Some(b'-') {
                    self.take()?;
                    target = true;
                }
                self.redirect = !target;
            }
            (Some(b'>'), Some(b'>' | b'|')) | (Some(b'<'), Some(b'>')) => {
                self.take()?;
                self.redirect = true;
            }
            _ => self.redirect = true,
        }
        Ok(self.lexeme(Some(TokenClass::Operator), start))
    }

    /// `(`: a function's `()`, a case pattern's, an array's, `((`, or a subshell.
    fn open_paren(&mut self, start: u32) -> Result<Lexeme, Failure> {
        let array = std::mem::take(&mut self.value);
        let command = self.state.pos == Pos::Command && !self.in_word;
        self.take()?;
        self.end_word();
        if self.state.pos == Pos::FuncOpen {
            self.state.pos = Pos::FuncClose;
        } else if self.state.frames.top() == Some(Frame::CasePattern) {
            // The optional `(` before a pattern.
        } else if array {
            self.state.frames.push(Frame::Group);
            self.state.pos = Pos::Args;
        } else if command && self.peek()? == Some(b'(') {
            self.take()?;
            self.state.frames.push(Frame::Arith);
        } else {
            self.state.frames.push(Frame::Group);
            self.state.pos = Pos::Command;
        }
        Ok(self.lexeme(Some(TokenClass::Punctuation), start))
    }

    /// `)`: the end of a function's `()`, a subshell, a `$(`, or a case pattern.
    fn close_paren(&mut self, start: u32) -> Result<Lexeme, Failure> {
        self.take()?;
        self.end_word();
        if self.state.pos == Pos::FuncClose {
            self.state.pos = Pos::Command;
            return Ok(self.lexeme(Some(TokenClass::Punctuation), start));
        }
        match self.state.frames.top() {
            Some(Frame::Group) => {
                self.state.frames.pop();
                self.state.pos = Pos::Args;
            }
            Some(Frame::Subst) => {
                self.state.frames.pop();
                self.state.pos = Pos::Args;
                // `$(a)b` is one word.
                self.in_word = true;
            }
            Some(Frame::CasePattern) => {
                self.state.frames.replace_top(Frame::CaseBody);
                self.state.pos = Pos::Command;
            }
            _ => {}
        }
        Ok(self.lexeme(Some(TokenClass::Punctuation), start))
    }

    /// A backtick: the end of a command substitution, or the start of one.
    pub(super) fn backtick(&mut self, start: u32, ctx: Ctx) -> Result<Lexeme, Failure> {
        if self.state.frames.top() == Some(Frame::Backtick) {
            self.take()?;
            self.state.frames.pop();
            self.state.pos = Pos::Args;
            self.in_word = true;
            return Ok(self.lexeme(Some(TokenClass::Punctuation), start));
        }
        if ctx == Ctx::Command {
            self.start_word();
        }
        self.take()?;
        self.state.frames.push(Frame::Backtick);
        self.state.pos = Pos::Command;
        self.end_word();
        Ok(self.lexeme(Some(TokenClass::Punctuation), start))
    }

    /// A plain piece of a word: a keyword, a builtin, a function's name, a number, an
    /// assignment's name - or nothing to colour.
    fn word(&mut self, start: u32) -> Result<Lexeme, Failure> {
        let role = self.start_word();
        if matches!(role, Role::At(Pos::Command | Pos::Decl)) {
            let found = words::assignment(self.reader.look(LOOK)?);
            if let Some((name, operator)) = found {
                return self.assignment(role, start, name, operator);
            }
        }
        let (text, len) = self.whole_word()?;
        let mut class = self.classify(role, &text[..len]);
        self.plain(start)?;
        let named = matches!(role, Role::At(Pos::Command | Pos::FuncName))
            && !matches!(
                class,
                Some(TokenClass::Keyword | TokenClass::Punctuation | TokenClass::Operator)
            );
        if named {
            let parens = len > 0 && words::function_parens(self.reader.look(LOOK)?);
            if parens {
                class = Some(TokenClass::Function);
                self.state.pos = Pos::FuncOpen;
            } else if role == Role::At(Pos::FuncName) {
                // `function name {`: the body's brace.
                self.state.pos = Pos::Command;
            }
        }
        Ok(self.lexeme(class, start))
    }

    /// The word ahead when it is plain and short (a keyword, a builtin, a number): its text and
    /// length; length 0 for any other.
    fn whole_word(&mut self) -> Result<([u8; WORD_BYTES], usize), Failure> {
        let head = self.reader.look(LOOK)?;
        let run = head
            .iter()
            .position(|byte| words::ends_plain(*byte))
            .unwrap_or(head.len());
        let ended = match head.get(run) {
            Some(byte) => words::is_meta(*byte),
            None => head.len() < LOOK,
        };
        let mut text = [0u8; WORD_BYTES];
        if !ended || run > WORD_BYTES {
            return Ok((text, 0));
        }
        text[..run].copy_from_slice(&head[..run]);
        Ok((text, run))
    }

    /// Takes a plain piece: up to a meta character, a quote, `$`, a backtick, a line
    /// continuation or the piece's limit; an escaped character is part of it.
    fn plain(&mut self, start: u32) -> Result<(), Failure> {
        while !self.full(start) {
            match self.peek()? {
                Some(b'\\') if !self.continues()? => self.escape()?,
                Some(byte) if !words::ends_plain(byte) => {
                    self.take()?;
                }
                _ => break,
            }
        }
        Ok(())
    }

    /// `name=`, `name+=`, `name[i]=`: the name as `key`, the operator next.
    fn assignment(
        &mut self,
        role: Role,
        start: u32,
        name: usize,
        operator: usize,
    ) -> Result<Lexeme, Failure> {
        let end = self.reader.offset() + name as u64;
        self.take_to(end)?;
        let key = self.lexeme(Some(TokenClass::Key), start);
        let at = self.col;
        self.take_n(operator)?;
        self.queued = Some(self.lexeme(Some(TokenClass::Operator), at));
        // An assignment before a command leaves the command's first word to come.
        if let Role::At(pos) = role {
            self.state.pos = pos;
        }
        self.value = true;
        Ok(key)
    }

    /// The class of a whole word `text` (empty when the word is not plain) in `role`, and where
    /// the word after it stands.
    fn classify(&mut self, role: Role, text: &[u8]) -> Option<TokenClass> {
        match role {
            Role::At(Pos::Command) => self.command_word(text),
            Role::At(Pos::FuncName) => Some(TokenClass::Function),
            Role::At(Pos::ForVar) => words::is_name(text).then_some(TokenClass::Key),
            Role::At(Pos::ForIn) => match text {
                b"in" => {
                    self.state.pos = Pos::Args;
                    Some(TokenClass::Keyword)
                }
                b"do" => {
                    self.state.pos = Pos::Command;
                    Some(TokenClass::Keyword)
                }
                _ => None,
            },
            Role::At(Pos::CaseIn) if text == b"in" => {
                self.state.frames.push(Frame::CasePattern);
                self.state.pos = Pos::Command;
                Some(TokenClass::Keyword)
            }
            Role::Pattern if text == b"esac" => {
                self.close_case();
                self.state.pos = Pos::Args;
                Some(TokenClass::Keyword)
            }
            Role::At(Pos::Args | Pos::Decl) | Role::Value => {
                if words::is_number(text) {
                    Some(TokenClass::Number)
                } else if text == b"]]" {
                    Some(TokenClass::Keyword)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// A command's first word: a keyword, a brace, `!`, a builtin, or a command of no colour.
    fn command_word(&mut self, text: &[u8]) -> Option<TokenClass> {
        if let Some(next) = words::keyword(text) {
            if text == b"esac" {
                self.close_case();
            }
            self.state.pos = next;
            return Some(TokenClass::Keyword);
        }
        match text {
            b"{" => {
                self.state.pos = Pos::Command;
                Some(TokenClass::Punctuation)
            }
            b"}" => Some(TokenClass::Punctuation),
            b"!" => {
                self.state.pos = Pos::Command;
                Some(TokenClass::Operator)
            }
            _ if words::is_builtin(text) => {
                if words::declares(text) {
                    self.state.pos = Pos::Decl;
                }
                Some(TokenClass::Function)
            }
            _ => None,
        }
    }

    /// `esac`: the case's frame closes.
    fn close_case(&mut self) {
        if matches!(
            self.state.frames.top(),
            Some(Frame::CasePattern | Frame::CaseBody)
        ) {
            self.state.frames.pop();
        }
    }
}
