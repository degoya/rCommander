//! CSV and TSV read into rows, and the rows written as a Markdown table.

/// The separators a CSV file is told apart by.
const CANDIDATES: [u8; 3] = [b',', b';', b'\t'];

/// The setting `separator` (`manifest.toml`, `[[settings]]`): the byte the person chose, or
/// `None` for `auto` - the file's own, as [`separator`] finds it.
#[must_use]
pub fn chosen_separator(setting: Option<&str>) -> Option<u8> {
    match setting {
        Some("comma") => Some(b','),
        Some("semicolon") => Some(b';'),
        Some("tab") => Some(b'\t'),
        _ => None,
    }
}

/// The separator of a file: a tab for `.tsv`, else the one of `,`, `;` and tab its first line
/// holds most often outside quotes; `None` when it holds none, which makes it no table.
#[must_use]
pub fn separator(name: &str, head: &[u8]) -> Option<u8> {
    let tsv = name
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("tsv"));
    if tsv {
        return Some(b'\t');
    }
    let mut counts = [0_usize; CANDIDATES.len()];
    let mut quoted = false;
    for byte in head {
        match byte {
            b'"' => quoted = !quoted,
            b'\n' if !quoted => break,
            _ if !quoted => {
                if let Some(index) = CANDIDATES.iter().position(|candidate| candidate == byte) {
                    counts[index] += 1;
                }
            }
            _ => {}
        }
    }
    let (index, count) = counts
        .iter()
        .enumerate()
        .max_by_key(|(index, count)| (**count, std::cmp::Reverse(*index)))?;
    (*count > 0).then_some(CANDIDATES[index])
}

/// Rows read from the start of a file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Rows {
    pub rows: Vec<Vec<String>>,
    /// The file goes on beyond these rows.
    pub more: bool,
}

/// Splits `bytes` into rows of fields as RFC 4180 has it: a field in double quotes may hold the
/// separator, line breaks and `""` for a quote; a line ends with LF or CRLF. `complete` is false
/// when `bytes` stop before the end of the file: the last row may be cut, so it is left out. At
/// most `max_rows` rows are read.
#[must_use]
pub fn rows(bytes: &[u8], separator: u8, complete: bool, max_rows: usize) -> Rows {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = Vec::new();
    let mut quoted = false;
    let mut started = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if quoted {
            if byte != b'"' {
                field.push(byte);
            } else if bytes.get(index) == Some(&b'"') {
                field.push(b'"');
                index += 1;
            } else {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' if !started => {
                quoted = true;
                started = true;
            }
            b'\n' => {
                if field.last() == Some(&b'\r') {
                    field.pop();
                }
                row.push(text(&mut field));
                rows.push(std::mem::take(&mut row));
                started = false;
                if rows.len() == max_rows {
                    return Rows {
                        rows,
                        more: index < bytes.len() || !complete,
                    };
                }
            }
            _ if byte == separator => {
                row.push(text(&mut field));
                started = false;
            }
            _ => {
                field.push(byte);
                started = true;
            }
        }
    }
    if complete && (started || !row.is_empty()) {
        row.push(text(&mut field));
        rows.push(row);
    }
    Rows {
        rows,
        more: !complete,
    }
}

fn text(field: &mut Vec<u8>) -> String {
    String::from_utf8_lossy(&std::mem::take(field)).into_owned()
}

/// The rows as a Markdown table, the first row its header. A short row is filled with empty
/// cells; `more` adds a line saying the table is the start of a larger file.
#[must_use]
pub fn markdown(rows: &Rows) -> String {
    let columns = rows.rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut out = String::new();
    for (number, row) in rows.rows.iter().enumerate() {
        out.push('|');
        for column in 0..columns {
            out.push(' ');
            escape(row.get(column).map_or("", String::as_str), &mut out);
            out.push_str(" |");
        }
        out.push('\n');
        if number == 0 {
            out.push('|');
            out.push_str(&" --- |".repeat(columns));
            out.push('\n');
        }
    }
    if rows.more {
        out.push_str(&format!(
            "\n_The first {} rows of a larger file._\n",
            rows.rows.len()
        ));
    }
    out
}

/// A cell's text as Markdown shows it literally: line breaks become spaces, and every character
/// that would format or end the cell is escaped.
fn escape(cell: &str, out: &mut String) {
    for character in cell.chars() {
        match character {
            '\r' | '\n' => out.push(' '),
            '\\' | '|' | '*' | '_' | '`' | '[' | ']' | '<' | '>' | '#' | '~' => {
                out.push('\\');
                out.push(character);
            }
            other => out.push(other),
        }
    }
}
