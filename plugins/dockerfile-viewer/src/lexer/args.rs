//! An instruction's arguments, by how they are read ([`Args`]): the exec form's JSON array,
//! `FROM`'s image and stage, `ENV`/`ARG`/`LABEL` pairs and the plain rest. The shell form is
//! [`super::shell`].

use super::cursor::{Cursor, is_blank};
use super::{Args, LineState, shell};
use crate::types::TokenClass;

/// The arguments of a line in `args`, from the cursor to the line's end.
pub(super) fn arguments(cursor: &mut Cursor<'_>, args: Args, state: &mut LineState) {
    match args {
        Args::Shell => shell::lex(cursor, state),
        Args::Json => json(cursor, state),
        Args::From => from(cursor),
        Args::Pairs => pairs(cursor, state),
        Args::Plain => plain(cursor, state),
    }
}

/// `RUN`, `CMD`, `ENTRYPOINT`: flags (`RUN --mount=...`), then the exec form when a `[` follows,
/// else the shell form.
pub(super) fn command(cursor: &mut Cursor<'_>, state: &mut LineState) -> Args {
    flags(cursor);
    if cursor.peek() == Some(b'[') {
        json(cursor, state);
        Args::Json
    } else {
        state.command_start = true;
        shell::lex(cursor, state);
        Args::Shell
    }
}

/// `HEALTHCHECK [flags] CMD ...` or `HEALTHCHECK NONE`.
pub(super) fn healthcheck(cursor: &mut Cursor<'_>, state: &mut LineState) -> Args {
    flags(cursor);
    let start = cursor.at;
    cursor.take_while(|byte| byte.is_ascii_alphabetic());
    let word = cursor.since(start).to_ascii_uppercase();
    match word.as_slice() {
        b"CMD" => {
            cursor.emit(start, TokenClass::Keyword);
            cursor.blanks();
            command(cursor, state)
        }
        b"NONE" => {
            cursor.emit(start, TokenClass::Keyword);
            plain(cursor, state);
            Args::Plain
        }
        _ => {
            cursor.at = start;
            plain(cursor, state);
            Args::Plain
        }
    }
}

/// The flags at the cursor, `--name=value` each, and the blanks after them.
fn flags(cursor: &mut Cursor<'_>) {
    loop {
        cursor.blanks();
        if !cursor.starts_with(b"--") {
            return;
        }
        cursor.flag();
    }
}

/// A quote the line before left open: its rest as a string. False while it stays open.
fn reopen(cursor: &mut Cursor<'_>, state: &mut LineState) -> bool {
    let Some(quote) = state.quote else {
        return true;
    };
    if !cursor.quoted(quote, false, quote == b'"') {
        return false;
    }
    state.quote = None;
    true
}

/// The exec form: `[`, `]` and `,` punctuation, the strings strings.
fn json(cursor: &mut Cursor<'_>, state: &mut LineState) {
    if !reopen(cursor, state) {
        return;
    }
    loop {
        cursor.blanks();
        let Some(byte) = cursor.peek() else {
            return;
        };
        match byte {
            b'[' | b']' | b',' => cursor.token(1, TokenClass::Punctuation),
            b'"' => {
                if !cursor.quoted(b'"', true, false) {
                    state.quote = Some(b'"');
                    return;
                }
            }
            _ => cursor.take_while(|byte| !is_blank(byte) && !b"[],\"".contains(&byte)),
        }
    }
}

/// `FROM [--platform=...] image [AS name]`: the image a type name, `AS` a keyword, the stage's
/// name a function (a later `--from=` names it).
fn from(cursor: &mut Cursor<'_>) {
    let mut position = 0;
    loop {
        cursor.blanks();
        if cursor.done() {
            return;
        }
        if cursor.starts_with(b"--") {
            cursor.flag();
            continue;
        }
        let start = cursor.at;
        match position {
            0 => {
                cursor.word(Some(TokenClass::TypeName));
            }
            1 => {
                cursor.take_while(|byte| !is_blank(byte));
                if cursor.since(start).eq_ignore_ascii_case(b"AS") {
                    cursor.emit(start, TokenClass::Keyword);
                }
            }
            2 => {
                cursor.word(Some(TokenClass::Function));
            }
            _ => {
                cursor.word(None);
            }
        }
        position += 1;
    }
}

/// `ENV`, `ARG`, `LABEL`: `key=value` pairs, a key alone (`ARG VERSION`), or `ENV key value` -
/// the old form, its value the rest of the line.
fn pairs(cursor: &mut Cursor<'_>, state: &mut LineState) {
    if !reopen(cursor, state) {
        return;
    }
    let mut first = true;
    loop {
        cursor.blanks();
        if cursor.done() {
            return;
        }
        let start = cursor.at;
        cursor.take_while(|byte| !is_blank(byte) && !matches!(byte, b'=' | b'"' | b'\'' | b'$'));
        if cursor.peek() == Some(b'=') {
            cursor.emit(start, TokenClass::Key);
            cursor.token(1, TokenClass::Operator);
            if let Some(quote) = cursor.word(Some(TokenClass::String)) {
                state.quote = Some(quote);
                return;
            }
        } else if cursor.at > start {
            cursor.emit(start, TokenClass::Key);
            if first && cursor.peek().is_some_and(is_blank) {
                // `ENV key value with spaces`: the rest is the value.
                cursor.blanks();
                while !cursor.done() {
                    if let Some(quote) = cursor.word(Some(TokenClass::String)) {
                        state.quote = Some(quote);
                        return;
                    }
                    cursor.blanks();
                }
                return;
            }
        } else if let Some(quote) = cursor.word(Some(TokenClass::String)) {
            // `LABEL "key"="value"`, `ENV $NAME=...`: no plain key.
            state.quote = Some(quote);
            return;
        }
        first = false;
    }
}

/// Everything else - `COPY`, `ADD`, `WORKDIR`, `USER`, `VOLUME`, `EXPOSE`, ...: flags, heredocs
/// (`COPY <<EOF /etc/x`), a JSON array, port numbers, variables and strings.
fn plain(cursor: &mut Cursor<'_>, state: &mut LineState) {
    if !reopen(cursor, state) {
        return;
    }
    loop {
        cursor.blanks();
        let Some(byte) = cursor.peek() else {
            return;
        };
        if cursor.starts_with(b"--") {
            cursor.flag();
        } else if cursor.starts_with(b"<<") {
            match cursor.heredoc(false) {
                Some(marker) => state.add_heredoc(marker),
                None => {
                    cursor.word(None);
                }
            }
        } else if byte == b'[' {
            json(cursor, state);
            return;
        } else if byte.is_ascii_digit() {
            let start = cursor.at;
            cursor.take_while(|byte| byte.is_ascii_digit());
            let ends = cursor
                .peek()
                .is_none_or(|next| is_blank(next) || matches!(next, b'/' | b':' | b'-'));
            if ends {
                cursor.emit(start, TokenClass::Number);
            }
            if let Some(quote) = cursor.word(None) {
                state.quote = Some(quote);
                return;
            }
        } else if let Some(quote) = cursor.word(None) {
            state.quote = Some(quote);
            return;
        }
    }
}
