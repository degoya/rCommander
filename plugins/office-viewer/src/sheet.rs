//! SpreadsheetML: the shared strings, and a sheet's rows as lines of tab-separated values. A
//! cell shows what Excel last calculated: the shared string its index names, an inline string,
//! `TRUE`/`FALSE` for a boolean, else the stored value - for a formula cell its cached result
//! (`<v>`), never the formula. Numbers are shown as stored (a date is its serial number: number
//! formats are not applied). A cell's column comes from its reference, so an empty cell between
//! two keeps its place. The first row with a value is the header; `max_rows` rows below it are
//! written, the rest counted in one closing line.

use std::io::Read;

use crate::out::Lines;
use crate::types::Failure;
use crate::xml::{Ev, Xml, attr, local, local_end};

/// The columns of a sheet: `A` to `XFD`.
const MAX_COLUMNS: u64 = 16_384;

/// All shared strings, one after another, and where each ends.
#[derive(Debug, Default)]
pub(crate) struct SharedStrings {
    text: String,
    ends: Vec<usize>,
}

impl SharedStrings {
    /// The string at `index`; empty for one that does not exist.
    pub(crate) fn get(&self, index: usize) -> &str {
        let start = match index.checked_sub(1) {
            Some(before) => self.ends.get(before).copied().unwrap_or(0),
            None => 0,
        };
        let end = self.ends.get(index).copied().unwrap_or(start);
        self.text.get(start..end).unwrap_or_default()
    }
}

/// Reads `xl/sharedStrings.xml`: each `si`, its runs joined, its phonetic guide (`rPh`) left
/// out.
pub(crate) fn shared_strings<R: Read>(xml: &mut Xml<R>) -> Result<SharedStrings, Failure> {
    let mut strings = SharedStrings::default();
    let (mut in_text, mut phonetic) = (false, 0usize);
    loop {
        match xml.next()? {
            Ev::Start(start) => match local(&start) {
                b"t" => in_text = true,
                b"rPh" => phonetic += 1,
                _ => {}
            },
            Ev::End(end) => match local_end(&end) {
                b"t" => in_text = false,
                b"rPh" => phonetic = phonetic.saturating_sub(1),
                b"si" => strings.ends.push(strings.text.len()),
                _ => {}
            },
            Ev::Empty(start) if local(&start) == b"si" => strings.ends.push(strings.text.len()),
            Ev::Text(text) if in_text && phonetic == 0 => strings.text.push_str(&text),
            Ev::Eof => return Ok(strings),
            _ => {}
        }
    }
}

/// What a sheet holds: rows with a value, and the widest of them in columns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Size {
    pub(crate) rows: u64,
    pub(crate) columns: u64,
}

/// The kind of value a cell stores (`t`).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Shared,
    Bool,
    Plain,
}

/// The cell being read.
struct Cell {
    column: Option<u64>,
    kind: Kind,
    value: String,
}

/// Writes the rows of one sheet into `out`: the header and at most `max_rows` rows below it,
/// then `[... n more rows]` for the rest. Returns the sheet's size.
pub(crate) fn write<R: Read>(
    xml: &mut Xml<R>,
    strings: &SharedStrings,
    max_rows: u64,
    out: &mut dyn Lines,
) -> Result<Size, Failure> {
    let mut size = Size::default();
    let mut line = String::new();
    let mut next_column = 0u64;
    let mut cell: Option<Cell> = None;
    let (mut in_value, mut in_inline, mut in_text, mut phonetic) = (false, false, false, 0usize);
    loop {
        match xml.next()? {
            Ev::Start(start) => match local(&start) {
                b"row" => {
                    line.clear();
                    next_column = 0;
                }
                b"c" => {
                    cell = Some(Cell {
                        column: attr(&start, b"r").as_deref().and_then(column_of),
                        kind: match attr(&start, b"t").as_deref() {
                            Some("s") => Kind::Shared,
                            Some("b") => Kind::Bool,
                            _ => Kind::Plain,
                        },
                        value: String::new(),
                    });
                }
                b"v" => in_value = true,
                b"is" => in_inline = true,
                b"t" => in_text = true,
                b"rPh" => phonetic += 1,
                _ => {}
            },
            Ev::End(end) => match local_end(&end) {
                b"v" => in_value = false,
                b"is" => in_inline = false,
                b"t" => in_text = false,
                b"rPh" => phonetic = phonetic.saturating_sub(1),
                b"c" => {
                    if let Some(done) = cell.take() {
                        place(&mut line, &mut next_column, &done, strings);
                    }
                }
                b"row" => {
                    if next_column == 0 {
                        continue;
                    }
                    size.rows += 1;
                    size.columns = size.columns.max(next_column);
                    // The header, then `max_rows` rows.
                    if size.rows <= max_rows.saturating_add(1) && !out.full() {
                        line.push('\n');
                        out.push(&line)?;
                    }
                }
                _ => {}
            },
            Ev::Text(text) => {
                if let Some(open) = cell.as_mut()
                    && (in_value || (in_inline && in_text && phonetic == 0))
                {
                    open.value.push_str(&text);
                }
            }
            Ev::Eof => break,
            Ev::Empty(_) | Ev::Other => {}
        }
    }
    let more = size.rows.saturating_sub(max_rows.saturating_add(1));
    if more > 0 {
        out.push(&format!("[... {more} more rows]\n"))?;
    }
    Ok(size)
}

/// Puts the value of `cell` into the row's `line` at its column, tabs before it for the columns
/// in between; an empty value leaves the line as it is.
fn place(line: &mut String, next_column: &mut u64, cell: &Cell, strings: &SharedStrings) {
    let value = match cell.kind {
        Kind::Shared => cell
            .value
            .trim()
            .parse::<usize>()
            .map_or("", |index| strings.get(index)),
        Kind::Bool => match cell.value.trim() {
            "1" => "TRUE",
            "0" => "FALSE",
            other => other,
        },
        Kind::Plain => cell.value.as_str(),
    };
    if value.is_empty() {
        return;
    }
    let column = cell
        .column
        .filter(|column| *column >= *next_column)
        .unwrap_or(*next_column);
    // At most MAX_COLUMNS: `column_of` refuses anything past `XFD`.
    let tabs = usize::try_from(column - *next_column).unwrap_or(0) + usize::from(*next_column > 0);
    line.extend(std::iter::repeat_n('\t', tabs));
    // A value is one field of its line: its own tabs and line breaks become spaces.
    line.extend(value.chars().map(|character| {
        if matches!(character, '\t' | '\n' | '\r') {
            ' '
        } else {
            character
        }
    }));
    *next_column = column + 1;
}

/// The size a sheet declares in `<dimension ref="A1:D20">` before its rows; else the rows are
/// counted. A sheet with no value has no size.
pub(crate) fn size<R: Read>(xml: &mut Xml<R>) -> Result<Option<Size>, Failure> {
    loop {
        match xml.next()? {
            Ev::Start(start) | Ev::Empty(start) => match local(&start) {
                b"dimension" => {
                    if let Some(size) = attr(&start, b"ref").as_deref().and_then(dimension) {
                        return Ok(Some(size));
                    }
                }
                b"sheetData" => break,
                _ => {}
            },
            Ev::Eof => return Ok(None),
            _ => {}
        }
    }
    let counted = write(
        xml,
        &SharedStrings::default(),
        u64::MAX,
        &mut crate::out::Discard,
    )?;
    Ok((counted.rows > 0).then_some(counted))
}

/// The zero-based column of a cell reference such as `BC12`; `None` past `XFD`.
pub(crate) fn column_of(reference: &str) -> Option<u64> {
    let letters: Vec<u8> = reference
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .collect();
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    let column = letters.iter().fold(0u64, |column, letter| {
        column * 26 + u64::from(letter.to_ascii_uppercase() - b'A' + 1)
    }) - 1;
    (column < MAX_COLUMNS).then_some(column)
}

/// The one-based row of a cell reference such as `BC12`.
fn row_of(reference: &str) -> Option<u64> {
    reference
        .trim_start_matches(|character: char| character.is_ascii_alphabetic())
        .parse()
        .ok()
        .filter(|row| *row > 0)
}

/// The size a `dimension` reference spans: `A1:D20` is 20 rows of 4 columns, `A1` one cell.
fn dimension(reference: &str) -> Option<Size> {
    let (first, last) = reference.split_once(':').unwrap_or((reference, reference));
    let (first_column, last_column) = (column_of(first)?, column_of(last)?);
    let (first_row, last_row) = (row_of(first)?, row_of(last)?);
    Some(Size {
        rows: last_row.checked_sub(first_row)? + 1,
        columns: last_column.checked_sub(first_column)? + 1,
    })
}

#[cfg(test)]
#[path = "sheet_tests.rs"]
mod tests;
