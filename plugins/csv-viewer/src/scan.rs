//! The open pass: the file read once, in windows of at most [`crate::OPEN_CHUNK`], through the
//! CSV machine. It keeps the two indexes, notes a quote never closed and, for `auto`, whether the
//! file is UTF-8 - nothing else of the file stays.

use crate::OPEN_CHUNK;
use crate::encoding::Utf8Check;
use crate::lines::{LineIndex, LineState, RecordIndex};
use crate::machine::{Kind, Machine, Mode};
use crate::types::{Failure, FailureKind, Source};

/// What the open pass learnt.
#[derive(Debug)]
pub(crate) struct Scan {
    pub(crate) lines: LineIndex,
    pub(crate) records: RecordIndex,
    /// The line of a quote still open at the end of the file: its field is broken from there on.
    pub(crate) unclosed_line: Option<u64>,
    /// Whether the file is valid UTF-8; true when it was not asked.
    pub(crate) utf8: bool,
}

/// Reads the file from `start` (after a byte-order mark) to its end.
pub(crate) fn scan<S: Source>(
    src: &mut S,
    start: u64,
    sep: u8,
    check_utf8: bool,
) -> Result<Scan, Failure> {
    let size = src.size();
    let mut lines = LineIndex::new(start);
    let mut records = RecordIndex::new(start);
    let mut machine = Machine::new(sep, Mode::RecordStart);
    let mut check = Utf8Check::default();
    let (mut line, mut record_count, mut column, mut quote_line) = (0u64, 0u64, 0u32, 0u64);
    let mut last = None;
    let mut at = start;
    while at < size {
        if src.should_stop() {
            return Err(Failure::of(FailureKind::Cancelled));
        }
        let length = u32::try_from((size - at).min(u64::from(OPEN_CHUNK))).unwrap_or(OPEN_CHUNK);
        let bytes = src.read_at(at, length)?;
        let window = bytes.get(..length as usize).unwrap_or(&bytes);
        if window.is_empty() {
            // A source shorter than announced ends here rather than looping.
            break;
        }
        for (index, byte) in window.iter().enumerate() {
            let offset = at + index as u64;
            if check_utf8 {
                check.feed(*byte);
            }
            let before = machine.mode;
            let step = machine.feed(*byte);
            if step.began {
                records.start(record_count, offset, ());
                record_count += 1;
            }
            match step.kind {
                Kind::End { record: true, .. } => column = 0,
                Kind::End { record: false, .. } => column = column.saturating_add(1),
                Kind::Content(None)
                    if machine.mode == Mode::Quoted
                        && matches!(before, Mode::RecordStart | Mode::FieldStart) =>
                {
                    quote_line = line;
                }
                _ => {}
            }
            if *byte == b'\n' {
                line += 1;
                let quoted = machine.mode == Mode::Quoted;
                lines.start(line, offset + 1, LineState { quoted, column });
            }
        }
        at += window.len() as u64;
        last = window.last().copied();
        src.progress(at, Some(size));
    }
    let unclosed_line = (machine.mode == Mode::Quoted).then_some(quote_line);
    // A last line without a line break counts, an empty one after the last break does not.
    let line_count = if at > start && last != Some(b'\n') {
        line + 1
    } else {
        line
    };
    lines.finish(line_count);
    records.finish(record_count);
    Ok(Scan {
        lines,
        records,
        unclosed_line,
        utf8: !check_utf8 || check.valid(),
    })
}
