//! LaTeX, for documents (`.tex`) and packages (`.sty`): commands (`@` a letter, as in a package),
//! `%` comments, braces and brackets, math in `$...$`, `$$...$$`, `\(...\)` and `\[...\]` (its
//! text a string, its commands keywords; a blank line ends it, as TeX does), the environment
//! named by `\begin` and `\end`, packages and classes, the keys of `\label`, `\ref` and `\cite`,
//! files, the command a definition names, `#1` parameters, verbatim environments and `\verb`.

use super::{Block, Cursor, Emit, MAX_INLINE, MathClose, Tag};
use crate::types::TokenClass;

/// What the brace after a command holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arg {
    /// An environment's name; `begin`: where `\begin` stands, should it open a verbatim one.
    Environment { begin: Option<usize> },
    /// A label, reference or citation key.
    Key,
    /// A package or class.
    Package,
    /// A file.
    File,
}

/// The environments whose text is not LaTeX, up to their `\end`.
const VERBATIM: &[&[u8]] = &[
    b"Verbatim",
    b"comment",
    b"lstlisting",
    b"minted",
    b"verbatim",
    b"verbatim*",
];

/// Lexes one line of LaTeX that starts in `block`; returns the block the next line starts in.
pub(crate) fn line<E: Emit>(cur: &mut Cursor<'_, E>, block: Block) -> Block {
    let mut block = block;
    if matches!(block, Block::Math(_)) && cur.line.trim_ascii().is_empty() {
        cur.failed();
        return Block::Text;
    }
    let mut arg = None;
    let mut define = false;
    while cur.pos < cur.end() {
        block = match block {
            Block::Math(close) => math(cur, close),
            Block::Verbatim { tag, comment } => verbatim(cur, tag, comment),
            _ => text(cur, &mut arg, &mut define).unwrap_or(Block::Text),
        };
    }
    block
}

/// One step of text: a comment, a command, math opening, a bracket, an operator, or a run of
/// plain text. Returns the block it opened: math, or a verbatim environment.
fn text<E: Emit>(
    cur: &mut Cursor<'_, E>,
    arg: &mut Option<Arg>,
    define: &mut bool,
) -> Option<Block> {
    let start = cur.pos;
    let byte = cur.byte(start)?;
    match byte {
        b'%' => cur.rest(TokenClass::Comment),
        b'\\' => return command(cur, arg, define),
        b'$' => {
            let (width, close) = if cur.byte(start + 1) == Some(b'$') {
                (2, MathClose::Dollars)
            } else {
                (1, MathClose::Dollar)
            };
            cur.take(start, start + width, TokenClass::Operator);
            cur.opened(start, width);
            return Some(Block::Math(close));
        }
        b'{' if arg.is_some() => return argument(cur, arg),
        b'[' if arg.is_some() => match cur.find(start, b']', MAX_INLINE) {
            // An optional argument before the one in braces: `\cite[p. 5]{key}`.
            Some(close) => {
                cur.take(start, start + 1, TokenClass::Punctuation);
                cur.take(close, close + 1, TokenClass::Punctuation);
            }
            None => {
                *arg = None;
                cur.take(start, start + 1, TokenClass::Punctuation);
            }
        },
        b'{' | b'}' | b'[' | b']' => {
            *arg = None;
            cur.take(start, start + 1, TokenClass::Punctuation);
        }
        b'&' | b'~' | b'^' | b'_' => cur.take(start, start + 1, TokenClass::Operator),
        b'#' if cur.byte(start + 1).is_some_and(|b| b.is_ascii_digit()) => {
            cur.take(start, start + 2, TokenClass::Key);
        }
        b' ' | b'\t' => cur.pos = cur.blank_end(start),
        _ => {
            *arg = None;
            let mut at = start + 1;
            while cur
                .byte(at)
                .is_some_and(|b| !b"\\%${}[]&~^_# \t".contains(&b))
            {
                at += 1;
            }
            cur.pos = at;
        }
    }
    None
}

/// A command at the cursor: a control word, a control symbol, math's `\(` and `\[`, `\verb`.
/// Returns the math it opened.
fn command<E: Emit>(
    cur: &mut Cursor<'_, E>,
    arg: &mut Option<Arg>,
    define: &mut bool,
) -> Option<Block> {
    let start = cur.pos;
    let mut end = start + 1;
    while cur
        .byte(end)
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'@')
    {
        end += 1;
    }
    if end == start + 1 {
        // A control symbol: `\\`, `\%`, `\{`; `\(` and `\[` open math.
        // The symbol is one ASCII byte; a character beyond ASCII is text after a lone `\`.
        let width = if cur.byte(end).is_some_and(|b| b.is_ascii()) {
            2
        } else {
            1
        };
        cur.take(start, start + width, TokenClass::Operator);
        let close = match cur.byte(end) {
            Some(b'(') => MathClose::Paren,
            Some(b'[') => MathClose::Bracket,
            _ => return None,
        };
        cur.opened(start, 2);
        return Some(Block::Math(close));
    }
    let line = cur.line;
    let name = &line[start + 1..end];
    let starred = cur.byte(end) == Some(b'*');
    let class = if std::mem::take(define) {
        TokenClass::Function
    } else {
        TokenClass::Keyword
    };
    *arg = match name {
        b"begin" => Some(Arg::Environment { begin: Some(start) }),
        b"end" | b"newenvironment" | b"renewenvironment" => Some(Arg::Environment { begin: None }),
        b"label" | b"ref" | b"eqref" | b"pageref" | b"autoref" | b"cref" | b"Cref" | b"nameref"
        | b"cite" | b"citep" | b"citet" | b"citeauthor" | b"citeyear" | b"nocite"
        | b"parencite" | b"textcite" | b"autocite" | b"footcite" | b"bibitem" => Some(Arg::Key),
        b"usepackage" | b"RequirePackage" | b"documentclass" | b"LoadClass" => Some(Arg::Package),
        b"input" | b"include" | b"includegraphics" | b"bibliography" | b"addbibresource" => {
            Some(Arg::File)
        }
        _ => None,
    };
    *define = matches!(
        name,
        b"newcommand"
            | b"renewcommand"
            | b"providecommand"
            | b"DeclareRobustCommand"
            | b"DeclareMathOperator"
            | b"NewDocumentCommand"
            | b"RenewDocumentCommand"
            | b"def"
            | b"gdef"
            | b"edef"
            | b"xdef"
            | b"let"
    );
    let verb = name == b"verb";
    let end = end + usize::from(starred);
    cur.take(start, end, class);
    if verb {
        verb_text(cur);
    }
    None
}

/// `\verb|text|`: the delimiter after `\verb` closes it on the same line.
fn verb_text<E: Emit>(cur: &mut Cursor<'_, E>) {
    let start = cur.pos;
    let Some(delimiter) = cur.byte(start) else {
        return;
    };
    let end = cur
        .find(start + 1, delimiter, MAX_INLINE)
        .map_or(cur.end(), |close| close + 1);
    cur.take(start, end, TokenClass::String);
}

/// The braces of an argument whose content has a class of its own: `{document}`, `{fig:1}`.
/// Returns the verbatim environment `\begin{verbatim}` opens.
fn argument<E: Emit>(cur: &mut Cursor<'_, E>, arg: &mut Option<Arg>) -> Option<Block> {
    let start = cur.pos;
    let kind = arg.take();
    let Some(close) = cur.find(start + 1, b'}', MAX_INLINE) else {
        cur.take(start, start + 1, TokenClass::Punctuation);
        return None;
    };
    let class = match kind {
        Some(Arg::Environment { .. }) => TokenClass::TypeName,
        Some(Arg::Key) => TokenClass::Key,
        Some(Arg::Package) => TokenClass::TypeName,
        _ => TokenClass::String,
    };
    cur.push(start, start + 1, TokenClass::Punctuation);
    cur.push(start + 1, close, class);
    cur.take(close, close + 1, TokenClass::Punctuation);
    let line = cur.line;
    let name = &line[start + 1..close];
    let Some(Arg::Environment { begin: Some(begin) }) = kind else {
        return None;
    };
    if !VERBATIM.contains(&name) {
        return None;
    }
    let tag = Tag::new(name)?;
    cur.opened(begin, 6);
    Some(Block::Verbatim {
        comment: name == b"comment",
        tag,
    })
}

/// A verbatim environment's text from the cursor, up to `\end{tag}`: a string, or a comment in
/// the `comment` environment.
fn verbatim<E: Emit>(cur: &mut Cursor<'_, E>, tag: Tag, comment: bool) -> Block {
    let class = if comment {
        TokenClass::Comment
    } else {
        TokenClass::String
    };
    let start = cur.pos;
    let line = cur.line;
    let mut at = start;
    while let Some(found) = line
        .get(at..)
        .and_then(|rest| rest.windows(5).position(|window| window == b"\\end{"))
    {
        let end = at + found;
        if line.get(end + 5..).is_some_and(|rest| {
            rest.starts_with(tag.bytes()) && rest.get(tag.bytes().len()) == Some(&b'}')
        }) {
            cur.push(start, end, class);
            cur.pos = end;
            cur.closed();
            return Block::Text;
        }
        at = end + 5;
    }
    cur.rest(class);
    Block::Verbatim { tag, comment }
}

/// Math from the cursor: its text a string, commands keywords, braces and `^ _ &` themselves, up
/// to its closing delimiter.
fn math<E: Emit>(cur: &mut Cursor<'_, E>, close: MathClose) -> Block {
    let mut seg = cur.pos;
    let mut at = cur.pos;
    while let Some(byte) = cur.byte(at) {
        let closing = match (byte, close) {
            (b'$', MathClose::Dollar) => Some(1),
            (b'$', MathClose::Dollars) if cur.byte(at + 1) == Some(b'$') => Some(2),
            (b'\\', MathClose::Paren) if cur.byte(at + 1) == Some(b')') => Some(2),
            (b'\\', MathClose::Bracket) if cur.byte(at + 1) == Some(b']') => Some(2),
            _ => None,
        };
        if let Some(width) = closing {
            cur.push(seg, at, TokenClass::String);
            cur.take(at, at + width, TokenClass::Operator);
            cur.closed();
            return Block::Text;
        }
        let (end, class) = match byte {
            b'%' => (cur.end(), TokenClass::Comment),
            b'\\' => {
                let mut end = at + 1;
                while cur
                    .byte(end)
                    .is_some_and(|b| b.is_ascii_alphabetic() || b == b'@')
                {
                    end += 1;
                }
                if end == at + 1 {
                    let symbol = cur.byte(at + 1).is_some_and(|b| b.is_ascii());
                    (at + 1 + usize::from(symbol), TokenClass::Operator)
                } else {
                    (end, TokenClass::Keyword)
                }
            }
            b'{' | b'}' | b'[' | b']' | b'(' | b')' => (at + 1, TokenClass::Punctuation),
            b'^' | b'_' | b'&' | b'=' | b'+' | b'-' | b'<' | b'>' => (at + 1, TokenClass::Operator),
            _ => {
                at += 1;
                continue;
            }
        };
        cur.push(seg, at, TokenClass::String);
        cur.push(at, end, class);
        at = end;
        seg = end;
    }
    cur.push(seg, cur.end(), TokenClass::String);
    cur.pos = cur.end();
    Block::Math(close)
}

#[cfg(test)]
#[path = "latex_tests.rs"]
mod tests;
