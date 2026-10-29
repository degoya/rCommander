//! What `auto` decides from the head of the file: the separator and whether the first record is
//! a header. The encoding's `auto` needs the whole file and is decided by the open pass.

use crate::machine::{Kind, Machine, Mode};

/// The separators `auto` chooses from, in the order a tie goes.
pub const CANDIDATES: [u8; 4] = *b",;\t|";
/// The bytes of the head `auto` looks at.
pub const HEAD_BYTES: u32 = 64 * 1024;
/// The lines of the head `auto` counts separators on.
pub const HEAD_LINES: usize = 32;
/// The most fields of the header record kept as column names, and the bytes kept of each.
pub const MAX_NAMES: usize = 1024;
pub const NAME_BYTES: usize = 256;

/// The separator of a file under `auto`: a tab for `.tsv` and `.tab`, a pipe for `.psv`, else
/// the one of [`CANDIDATES`] found most often outside quotes on the first [`HEAD_LINES`] lines.
/// One found equally often on every line goes first: in a German Excel file every decimal comma
/// counts too, but only the semicolon is on every line the same number of times. None found: a
/// comma, and every line is one field.
#[must_use]
pub fn separator(name: &str, head: &[u8]) -> u8 {
    let extension = name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase());
    match extension.as_deref() {
        Some("tsv" | "tab") => return b'\t',
        Some("psv") => return b'|',
        _ => {}
    }
    let mut lines: Vec<[usize; CANDIDATES.len()]> = Vec::new();
    let mut counts = [0usize; CANDIDATES.len()];
    let mut quoted = false;
    let mut complete = true;
    for byte in head {
        complete = false;
        match byte {
            b'"' => quoted = !quoted,
            b'\n' if !quoted => {
                lines.push(std::mem::take(&mut counts));
                complete = true;
                if lines.len() == HEAD_LINES {
                    break;
                }
            }
            _ if !quoted => {
                if let Some(index) = CANDIDATES.iter().position(|candidate| candidate == byte) {
                    counts[index] += 1;
                }
            }
            _ => {}
        }
    }
    // A last line without its break counts when it is the only one or the file ends there.
    if !complete && (lines.is_empty() || head.len() < HEAD_BYTES as usize) {
        lines.push(counts);
    }
    let score = |index: usize| {
        let first = lines.first().map_or(0, |line| line[index]);
        let even = first > 0 && lines.iter().all(|line| line[index] == first);
        let total: usize = lines.iter().map(|line| line[index]).sum();
        (even, total)
    };
    let best = (0..CANDIDATES.len())
        .map(|index| (score(index), std::cmp::Reverse(index)))
        .max()
        .filter(|((_, total), _)| *total > 0);
    best.map_or(b',', |(_, std::cmp::Reverse(index))| CANDIDATES[index])
}

/// The fields of the first record in `head`, each its first [`NAME_BYTES`] bytes unquoted, at
/// most [`MAX_NAMES`] of them; a record longer than `head` ends with it.
#[must_use]
pub fn first_record(head: &[u8], sep: u8) -> Vec<Vec<u8>> {
    let mut machine = Machine::new(sep, Mode::RecordStart);
    let mut fields: Vec<Vec<u8>> = Vec::new();
    let mut field = Vec::new();
    let mut started = false;
    for byte in head {
        let step = machine.feed(*byte);
        started |= step.began;
        match step.kind {
            Kind::Blank => {}
            Kind::Content(Some(byte)) => {
                if field.len() < NAME_BYTES {
                    field.push(byte);
                }
            }
            Kind::Content(None) => {}
            Kind::End { record, .. } => {
                push_name(&mut fields, &mut field);
                if record {
                    return fields;
                }
            }
        }
    }
    if started {
        push_name(&mut fields, &mut field);
    }
    fields
}

fn push_name(fields: &mut Vec<Vec<u8>>, field: &mut Vec<u8>) {
    let mut name = std::mem::take(field);
    if name.last() == Some(&b'\r') {
        name.pop();
    }
    if fields.len() < MAX_NAMES {
        fields.push(name);
    }
}

/// Whether the first record is a header under `auto`: it has a field, and none of its fields is
/// a number.
#[must_use]
pub fn looks_like_header(fields: &[Vec<u8>]) -> bool {
    !fields.is_empty() && !fields.iter().any(|field| is_number(field))
}

/// A number as a spreadsheet writes one: an optional sign, digits with at most one decimal point
/// or comma, an optional exponent; spaces around it allowed.
#[must_use]
pub fn is_number(field: &[u8]) -> bool {
    let text = field.trim_ascii();
    let unsigned = text
        .strip_prefix(b"-")
        .or_else(|| text.strip_prefix(b"+"))
        .unwrap_or(text);
    let (mantissa, exponent) = match unsigned.iter().position(|byte| matches!(byte, b'e' | b'E')) {
        Some(at) => (
            unsigned.get(..at).unwrap_or_default(),
            unsigned.get(at + 1..),
        ),
        None => (unsigned, None),
    };
    let digits = |part: &[u8]| part.iter().all(u8::is_ascii_digit);
    let (whole, fraction) = match mantissa.iter().position(|byte| matches!(byte, b'.' | b',')) {
        Some(at) => (
            mantissa.get(..at).unwrap_or_default(),
            mantissa.get(at + 1..),
        ),
        None => (mantissa, None),
    };
    let mantissa_ok = digits(whole)
        && fraction.is_none_or(digits)
        && (!whole.is_empty() || fraction.is_some_and(|fraction| !fraction.is_empty()));
    let exponent_ok = exponent.is_none_or(|exponent| {
        let exponent = exponent
            .strip_prefix(b"-")
            .or_else(|| exponent.strip_prefix(b"+"))
            .unwrap_or(exponent);
        !exponent.is_empty() && digits(exponent)
    });
    mantissa_ok && exponent_ok
}

#[cfg(test)]
#[path = "detect_tests.rs"]
mod tests;
