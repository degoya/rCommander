//! The lines of checksum files: written in the `sha256sum`/`md5sum` form (`<hex> *<name>`) or
//! as SFV (`<name> <CRC32>`), read in those two and the BSD form (`SHA256 (name) = hex`). The
//! format is recognised per line by its content; the file's extension only breaks the one tie
//! (64 hex digits: SHA-256 or BLAKE3) and decides whether SFV is tried first.

use serde::{Deserialize, Serialize};

use crate::Algorithm;

/// The largest checksum file [`crate::verify`] reads: about 150 000 SHA-256 lines.
pub const MAX_CHECKSUM_FILE: u64 = 16 * 1024 * 1024;

/// One parsed line: the name as the file writes it (not yet checked to stay in its folder),
/// the algorithm and the expected digest in lowercase hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksumLine {
    pub name: String,
    pub algorithm: Algorithm,
    pub hex: String,
}

/// Every line of `text` that is not blank or a comment (`;` or `#`): its 1-based number and the
/// parsed line, or `None` when it is in no known format. `hint` is the algorithm the file's
/// extension names.
#[must_use]
pub fn parse_checksums(text: &str, hint: Option<Algorithm>) -> Vec<(usize, Option<ChecksumLine>)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            let trimmed = line.trim_start();
            if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
                return None;
            }
            Some((index + 1, parse_line(line, hint)))
        })
        .collect()
}

fn parse_line(line: &str, hint: Option<Algorithm>) -> Option<ChecksumLine> {
    if hint == Some(Algorithm::Crc32) {
        sfv(line).or_else(|| bsd(line)).or_else(|| gnu(line, hint))
    } else {
        bsd(line).or_else(|| gnu(line, hint)).or_else(|| sfv(line))
    }
}

/// `<hex> *<name>` or `<hex>  <name>`; `\` before the line marks an escaped name.
fn gnu(line: &str, hint: Option<Algorithm>) -> Option<ChecksumLine> {
    let (escaped, line) = match line.strip_prefix('\\') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let (hex, rest) = line.split_once(' ')?;
    let algorithm = match hint {
        Some(hint) if hint.hex_len() == hex.len() => hint,
        _ => Algorithm::from_hex_len(hex.len())?,
    };
    if !is_hex(hex) {
        return None;
    }
    let name = rest
        .strip_prefix('*')
        .or_else(|| rest.strip_prefix(' '))
        .unwrap_or(rest);
    let name = if escaped {
        unescape(name)?
    } else {
        name.to_owned()
    };
    Some(ChecksumLine {
        name,
        algorithm,
        hex: hex.to_ascii_lowercase(),
    })
}

/// `<TAG> (<name>) = <hex>`.
fn bsd(line: &str) -> Option<ChecksumLine> {
    let (escaped, line) = match line.strip_prefix('\\') {
        Some(rest) => (true, rest),
        None => (false, line),
    };
    let (tag, rest) = line.split_once(" (")?;
    let algorithm = Algorithm::from_tag(tag)?;
    let (name, hex) = rest.rsplit_once(") = ")?;
    if hex.len() != algorithm.hex_len() || !is_hex(hex) {
        return None;
    }
    let name = if escaped {
        unescape(name)?
    } else {
        name.to_owned()
    };
    Some(ChecksumLine {
        name,
        algorithm,
        hex: hex.to_ascii_lowercase(),
    })
}

/// `<name> <CRC32>`: the last word is eight hex digits, the rest is the name.
fn sfv(line: &str) -> Option<ChecksumLine> {
    let line = line.trim_end();
    let (name, hex) = line.rsplit_once([' ', '\t'])?;
    let name = name.trim_end();
    if name.is_empty() || hex.len() != 8 || !is_hex(hex) {
        return None;
    }
    Some(ChecksumLine {
        name: name.to_owned(),
        algorithm: Algorithm::Crc32,
        hex: hex.to_ascii_lowercase(),
    })
}

fn is_hex(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// GNU's escaping: `\\` is a backslash, `\n` a newline, `\r` a carriage return; any other
/// escape makes the line invalid.
fn unescape(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                '\\' => out.push('\\'),
                'n' => out.push('\n'),
                'r' => out.push('\r'),
                _ => return None,
            }
        } else {
            out.push(c);
        }
    }
    Some(out)
}

/// One line of a checksum file for `name` (relative, `/`-separated), with its newline: SFV for
/// CRC32, `<hex> *<name>` otherwise - with GNU's escaping when the name holds a backslash or a
/// line break, as `sha256sum` writes it.
#[must_use]
pub fn sum_line(algorithm: Algorithm, hex: &str, name: &str) -> String {
    if algorithm == Algorithm::Crc32 {
        return format!("{name} {}\n", hex.to_ascii_uppercase());
    }
    if name.contains(['\\', '\n', '\r']) {
        let escaped = name
            .replace('\\', "\\\\")
            .replace('\n', "\\n")
            .replace('\r', "\\r");
        format!("\\{hex} *{escaped}\n")
    } else {
        format!("{hex} *{name}\n")
    }
}

/// What starts a checksum file: a comment line for SFV, nothing for the others (`md5sum -c`
/// would warn about a comment).
#[must_use]
pub fn sfv_header(algorithm: Algorithm) -> &'static str {
    if algorithm == Algorithm::Crc32 {
        "; Generated by rCommander\n"
    } else {
        ""
    }
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
