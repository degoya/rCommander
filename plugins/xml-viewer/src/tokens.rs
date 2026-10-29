//! The tokens of one line for the `highlighted` output.
//!
//! A line is lexed on its own, from the mode the line index or the line before hands it, so a
//! comment or a CDATA section that began lines earlier is still coloured as one. Every span starts
//! and ends at an ASCII byte or the line's end, so it sits on character boundaries whatever the
//! line holds. Text between markup gets no token: it is shown in the text's own colour.

use crate::lexer::{Kind, LexState, Lexeme, Lexer, Mode};
use crate::reader::SliceBytes;
use crate::types::{Token, TokenClass};

/// The most tokens one line gets (the host's rule, `docs/plugins.md` "Validation of plugin
/// output"); the rest of the line stays uncoloured.
pub const MAX_TOKENS_PER_LINE: usize = 4096;

/// `]]>`, the end of a closed CDATA piece.
const CDATA_CLOSE: u32 = 3;

/// The tokens of `line` (without its line break), lexed from `mode`, and the mode the line ends
/// in. `capped`: the line goes on after these bytes, so a lexeme touching the end may be cut and
/// is left out, and the mode it ends in means nothing.
pub(crate) fn tokenize_line(line: &[u8], mode: Mode, capped: bool) -> (Vec<Token>, Mode) {
    let mut lexer = Lexer::resume(SliceBytes::new(line), LexState::at(mode));
    let mut tokens = Vec::new();
    // A line in memory cannot fail to read.
    while let Ok(Some(lexeme)) = lexer.next() {
        if capped && lexeme.end == line.len() as u64 {
            break;
        }
        push(&mut tokens, &lexeme);
    }
    (tokens, lexer.mode())
}

/// The mode a whole line read from `mode` ends in.
pub(crate) fn end_mode(line: &[u8], mode: Mode) -> Mode {
    let mut lexer = Lexer::resume(SliceBytes::new(line), LexState::at(mode));
    while let Ok(Some(_)) = lexer.next() {}
    lexer.mode()
}

/// The tokens of one lexeme, while the line has room: none for text, two for a closed CDATA
/// piece (its content and the `]]>`), one for anything else.
fn push(tokens: &mut Vec<Token>, lexeme: &Lexeme) {
    let (Ok(start), Ok(len)) = (
        u32::try_from(lexeme.start),
        u32::try_from(lexeme.end - lexeme.start),
    ) else {
        return;
    };
    let mut add = |start: u32, len: u32, class: TokenClass| {
        if len > 0 && tokens.len() < MAX_TOKENS_PER_LINE {
            tokens.push(Token { start, len, class });
        }
    };
    match lexeme.kind {
        Kind::CData { closed: true } => {
            let content = len.saturating_sub(CDATA_CLOSE);
            add(start, content, TokenClass::String);
            add(start + content, len - content, TokenClass::Punctuation);
        }
        kind => {
            if let Some(class) = class_of(kind) {
                add(start, len, class);
            }
        }
    }
}

/// The class of a lexeme; `None` for text.
fn class_of(kind: Kind) -> Option<TokenClass> {
    match kind {
        Kind::Text => None,
        Kind::Name => Some(TokenClass::TypeName),
        Kind::AttrName => Some(TokenClass::Key),
        Kind::Value { .. } | Kind::CData { .. } => Some(TokenClass::String),
        Kind::Entity => Some(TokenClass::Literal),
        Kind::Comment { .. } => Some(TokenClass::Comment),
        Kind::Pi { .. } | Kind::Doctype { .. } => Some(TokenClass::Keyword),
        Kind::Invalid(_) => Some(TokenClass::Invalid),
        Kind::Open
        | Kind::EndOpen
        | Kind::Eq
        | Kind::Close
        | Kind::EndClose
        | Kind::EmptyClose
        | Kind::CDataOpen => Some(TokenClass::Punctuation),
    }
}

#[cfg(test)]
#[path = "tokens_tests.rs"]
mod tests;
