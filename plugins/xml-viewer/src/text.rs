//! The text in both formats: whole for `text`, by line for `tokens`.

use crate::lexer::{LexState, Lexer, Mode};
use crate::lines::LineIndex;
use crate::pretty::{LayoutState, Piece, indentation};
use crate::reader::{Bytes, Line, Reader};
use crate::tokens::end_mode;
use crate::types::{Failure, Sink, Source};
use crate::utf8::TextWriter;

/// The bytes of one line `tokens` reads; the rest of a longer line stays uncoloured.
pub const MAX_LINE_BYTES: usize = 64 * 1024;
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

/// `pretty`: the XML laid out by [`LayoutState`].
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
                Piece::RawBreak => writer.push(b"\n")?,
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

/// `as_is`: lines `first..first + wanted`, each with the mode it starts in.
pub(crate) fn as_is_lines<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    first: u64,
    wanted: u64,
) -> Result<Vec<Line>, Failure> {
    let checkpoint = lines.at_or_before(first);
    let mut reader = Reader::new(src, checkpoint.offset, LINE_CHUNK);
    let (mut line, mut mode) = (checkpoint.line, checkpoint.lex.mode);
    let last = first + wanted;
    let mut out = Vec::new();
    while line < last {
        let start = reader.offset();
        let Some(mut read) = reader.read_line(MAX_LINE_BYTES)? else {
            break;
        };
        let next = if !read.capped {
            end_mode(&read.bytes, mode)
        } else if line + 1 >= last {
            mode
        } else if let Some(point) = lines.exact(line + 1) {
            // The rest of a long line: jump to the checkpoint after it.
            reader.seek(point.offset);
            point.lex.mode
        } else {
            let (next, offset) = read_through(reader.source(), start, mode)?;
            reader.seek(offset);
            next
        };
        if line >= first {
            read.mode = mode;
            out.push(read);
        }
        line += 1;
        mode = next;
    }
    Ok(out)
}

/// The mode the line starting at `start` in `mode` ends in, and where the next line starts: the
/// whole line lexed, for a long line no checkpoint follows.
fn read_through<S: Source>(src: &mut S, start: u64, mode: Mode) -> Result<(Mode, u64), Failure> {
    let mut lexer = Lexer::resume(Reader::new(src, start, LINE_CHUNK), LexState::at(mode));
    let mut next = None;
    while next.is_none() {
        let lexeme = lexer.next_with(&mut |_, offset, mode| {
            if next.is_none() {
                next = Some((mode, offset));
            }
        })?;
        if lexeme.is_none() {
            break;
        }
    }
    Ok(next.unwrap_or_else(|| (lexer.mode(), lexer.bytes_mut().offset())))
}

/// `pretty`: lines `first..first + wanted`, each with the mode it starts in, laid out again from
/// the nearest checkpoint.
pub(crate) fn pretty_lines<S: Source>(
    src: &mut S,
    lines: &LineIndex,
    first: u64,
    wanted: u64,
) -> Result<Vec<Line>, Failure> {
    let checkpoint = lines.at_or_before(first);
    let last = first + wanted;
    let mut lexer = Lexer::resume(
        Reader::new(src, checkpoint.offset, LINE_CHUNK),
        checkpoint.lex,
    )
    .with_capture(MAX_LINE_BYTES);
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
                Piece::Break(_) | Piece::RawBreak => {
                    if let Some(done) = current.take() {
                        out.push(done);
                    }
                    line += 1;
                    if line >= last {
                        break 'lexemes;
                    }
                    if line >= first {
                        let mut fresh = Line {
                            mode: lexeme.state.mode,
                            ..Line::default()
                        };
                        if let Piece::Break(depth) = piece {
                            append(&mut fresh, indentation(depth));
                        }
                        current = Some(fresh);
                    }
                }
                Piece::Space => {
                    if let Some(open) = current.as_mut() {
                        append(open, b" ");
                    }
                }
                Piece::Lexeme => {
                    if let Some(open) = current.as_mut() {
                        append(open, lexer.captured());
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

/// Appends `bytes` to `line` up to [`MAX_LINE_BYTES`]; a line that would grow past it is marked
/// capped.
fn append(line: &mut Line, bytes: &[u8]) {
    let room = MAX_LINE_BYTES.saturating_sub(line.bytes.len());
    if bytes.len() > room {
        line.capped = true;
    }
    line.bytes
        .extend_from_slice(bytes.get(..bytes.len().min(room)).unwrap_or_default());
}
