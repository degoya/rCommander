//! The text in both formats: whole for `text`, by line for `tokens`.

use crate::lexer::Lexer;
use crate::lines::LineIndex;
use crate::pretty::{LayoutState, Piece, indentation};
use crate::reader::{Line, Reader};
use crate::tokens::is_blank;
use crate::types::{Failure, Sink, Source};
use crate::utf8::TextWriter;

/// The bytes of one line `tokens` reads; the rest of a longer line stays uncoloured.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// Lines read past the last one asked for, to see whether a colon makes its last string a key.
const LOOKAHEAD_LINES: u64 = 4;
const LOOKAHEAD_BYTES: usize = 256;
/// The window `tokens` reads through.
const LINE_CHUNK: u32 = 64 * 1024;

/// `as_is`: the file from `start` (after a byte-order mark), as it is but for bytes that are not
/// UTF-8, written as U+FFFD.
pub(crate) fn copy<S: Source>(src: &mut S, start: u64, out: &mut impl Sink) -> Result<(), Failure> {
    let mut reader = Reader::new(src, start, crate::OPEN_CHUNK);
    let size = reader.source().size();
    let mut writer = TextWriter::new(out);
    reader.copy_range(start, size, &mut |bytes| writer.push(bytes))?;
    writer.finish()
}

/// `pretty`: the JSON laid out by [`LayoutState`].
pub(crate) fn pretty<S: Source>(
    src: &mut S,
    start: u64,
    out: &mut impl Sink,
) -> Result<(), Failure> {
    let mut writer = TextWriter::new(out);
    let mut lexer =
        Lexer::new(Reader::new(src, start, crate::OPEN_CHUNK)).with_capture(MAX_LINE_BYTES);
    let mut layout = LayoutState::default();
    while let Some(lexeme) = lexer.next()? {
        for piece in layout.feed(&lexeme).iter() {
            match piece {
                Piece::Break(depth) => {
                    writer.push(b"\n")?;
                    writer.push(indentation(depth))?;
                }
                Piece::Space => writer.push(b" ")?,
                Piece::Lexeme => {
                    writer.push(lexer.captured())?;
                    let kept = lexeme.start + lexer.captured().len() as u64;
                    if kept < lexeme.end {
                        // Longer than the capture: the rest straight from the source.
                        writer.flush()?;
                        lexer
                            .bytes_mut()
                            .copy_range(kept, lexeme.end, &mut |bytes| writer.push(bytes))?;
                    }
                }
            }
        }
    }
    writer.finish()
}

/// `as_is`: lines `first..first + wanted` and the look-ahead after them.
pub(crate) fn as_is_lines<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    first: u64,
    wanted: u64,
) -> Result<Vec<Line>, Failure> {
    let checkpoint = lines.at_or_before(first);
    let mut reader = Reader::new(src, checkpoint.offset, LINE_CHUNK);
    let mut line = checkpoint.line;
    while line < first {
        if !reader.skip_line()? {
            return Ok(Vec::new());
        }
        line += 1;
    }
    let last = first + wanted;
    let end = (last + LOOKAHEAD_LINES).min(lines.lines());
    let mut out = Vec::new();
    while line < end {
        let looking_ahead = line >= last;
        let cap = if looking_ahead {
            LOOKAHEAD_BYTES
        } else {
            MAX_LINE_BYTES
        };
        let Some(read) = reader.read_line(cap)? else {
            break;
        };
        line += 1;
        let (capped, blank) = (read.capped, is_blank(&read.bytes));
        out.push(read);
        if looking_ahead && !blank {
            break;
        }
        if capped && line < end {
            // The rest of a long line: jump to the checkpoint after it when there is one.
            match lines.exact(line) {
                Some(next) => reader.seek(next.offset),
                None => {
                    reader.skip_line()?;
                }
            }
        }
    }
    Ok(out)
}

/// `pretty`: lines `first..first + wanted` and the look-ahead after them, laid out again from
/// the nearest checkpoint.
pub(crate) fn pretty_lines<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    first: u64,
    wanted: u64,
) -> Result<Vec<Line>, Failure> {
    let checkpoint = lines.at_or_before(first);
    let last = first + wanted;
    let end = (last + LOOKAHEAD_LINES).min(lines.lines());
    let cap = |line: u64| {
        if line >= last {
            LOOKAHEAD_BYTES
        } else {
            MAX_LINE_BYTES
        }
    };
    let mut lexer =
        Lexer::new(Reader::new(src, checkpoint.offset, LINE_CHUNK)).with_capture(MAX_LINE_BYTES);
    let mut layout = checkpoint.state;
    // Line 0 starts with the file; any other checkpoint's lexeme still lays out the line before
    // its own until its first break.
    let (mut line, mut current) = if checkpoint.line == 0 {
        (0, (first == 0).then(Line::default))
    } else {
        (checkpoint.line - 1, None)
    };
    let mut out = Vec::new();
    'lexemes: while let Some(lexeme) = lexer.next()? {
        for piece in layout.feed(&lexeme).iter() {
            match piece {
                Piece::Break(depth) => {
                    if let Some(done) = current.take() {
                        let stop = line >= last && !is_blank(&done.bytes);
                        out.push(done);
                        if stop {
                            break 'lexemes;
                        }
                    }
                    line += 1;
                    if line >= end {
                        break 'lexemes;
                    }
                    if line >= first {
                        let mut fresh = Line::default();
                        append(&mut fresh, indentation(depth), cap(line));
                        current = Some(fresh);
                    }
                }
                Piece::Space => {
                    if let Some(open) = current.as_mut() {
                        append(open, b" ", cap(line));
                    }
                }
                Piece::Lexeme => {
                    if let Some(open) = current.as_mut() {
                        append(open, lexer.captured(), cap(line));
                        if lexeme.end - lexeme.start > lexer.captured().len() as u64 {
                            open.capped = true;
                        }
                    }
                }
            }
        }
    }
    if let Some(done) = current.take() {
        out.push(done);
    }
    Ok(out)
}

/// Appends `bytes` to `line` up to `cap`; a line that would grow past it is marked capped.
fn append(line: &mut Line, bytes: &[u8], cap: usize) {
    let room = cap.saturating_sub(line.bytes.len());
    if bytes.len() > room {
        line.capped = true;
    }
    line.bytes
        .extend_from_slice(bytes.get(..bytes.len().min(room)).unwrap_or_default());
}
