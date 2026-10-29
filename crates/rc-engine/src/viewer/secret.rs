//! Private keys never leave through MCP.
//!
//! The MCP mask (`rc-mcp`) hides credentials in prose - `password=...`, tokens, addresses - and,
//! through [`mask_private_keys`], every private key block in every answer, whatever the tool and
//! whether or not the file was allowed. `files.read_text` and `files.read_hex` also judge a small
//! file whole ([`KEY_FILE_MAX`]): a window may start in the middle of a key, where no marker
//! shows. In a larger file they look [`KEY_CONTEXT`] bytes around a window without a marker
//! ([`inside_key_block`]): a window of nothing but a key's body is masked whole.

use std::sync::LazyLock;

use rc_remote::key_markers;

/// What a private key block becomes.
pub const PLACEHOLDER: &str = "[private key redacted]";

/// Files up to this size are judged whole: an OpenSSH, PEM or PuTTY key file is far smaller.
pub const KEY_FILE_MAX: u64 = 16 * 1024;

/// Bytes before and after a window searched for the edges of a key block around it: more than
/// the body of any OpenSSH, PEM or PuTTY key.
pub const KEY_CONTEXT: u64 = 8 * 1024;

/// Markers that only private key material carries: the one table (RC-080-43).
const MARKERS: [&str; 4] = key_markers::PRIVATE_KEY_MARKERS;

/// Each marker in ASCII and in UTF-16 of either byte order, made once.
static PATTERNS: LazyLock<Vec<Vec<u8>>> = LazyLock::new(|| {
    MARKERS
        .iter()
        .flat_map(|marker| {
            let ascii = marker.as_bytes();
            let le = ascii.iter().flat_map(|&byte| [byte, 0]).collect();
            let be = ascii.iter().flat_map(|&byte| [0, byte]).collect();
            [ascii.to_vec(), le, be]
        })
        .collect()
});

/// Whether `bytes` carry a private key marker, in ASCII or UTF-16 of either byte order.
#[must_use]
pub fn has_private_key(bytes: &[u8]) -> bool {
    PATTERNS.iter().any(|pattern| {
        bytes
            .windows(pattern.len())
            .any(|window| window == pattern.as_slice())
    })
}

/// Whether `line` opens a private key block (`-----BEGIN ... PRIVATE KEY-----`, a PuTTY
/// `PuTTY-User-Key-File-` header) and whether it closes one (`-----END ... PRIVATE KEY-----`,
/// PuTTY's `Private-MAC:`).
fn block_edges(line: &str) -> (bool, bool) {
    let pem = line.contains(key_markers::PEM_PRIVATE);
    (
        (pem && line.contains(key_markers::PEM_ARMOUR)) || line.contains(key_markers::PPK_HEADER),
        (pem && line.contains("-----END")) || line.trim_start().starts_with(key_markers::PPK_MAC),
    )
}

/// Whether a window lies inside a private key block, judged by the text just `before` it and
/// just `after` it: an opening line before it that no line before it closes, or a closing line
/// after it before any opening one.
#[must_use]
pub fn inside_key_block(before: &str, after: &str) -> bool {
    let mut open = false;
    for line in before.split_inclusive('\n') {
        let (opens, closes) = block_edges(line);
        open = (open || opens) && !closes;
    }
    if open {
        return true;
    }
    for line in after.split_inclusive('\n') {
        let (opens, closes) = block_edges(line);
        if closes {
            return true;
        }
        if opens {
            return false;
        }
    }
    false
}

/// `text` with every private key block in it - its BEGIN and END lines included - replaced by
/// one [`PLACEHOLDER`] line; `None` when it holds none. A text that starts inside a block (a
/// closing line before any opening one) loses everything up to that line; one that ends inside
/// a block everything from its opening line.
#[must_use]
pub fn mask_private_keys(text: &str) -> Option<String> {
    if !key_markers::holds_private_key(text) {
        return None;
    }
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut hide = vec![false; lines.len()];
    let mut open: Option<usize> = None;
    let mut seen = false;
    for (index, line) in lines.iter().enumerate() {
        let (opens, closes) = block_edges(line);
        if opens && open.is_none() {
            open = Some(index);
            seen = true;
        }
        if closes {
            let from = match open.take() {
                Some(from) => from,
                None if !seen => 0,
                None => continue,
            };
            hide[from..=index].fill(true);
            seen = true;
        }
    }
    if let Some(from) = open {
        hide[from..].fill(true);
    }
    if !hide.contains(&true) {
        return None;
    }
    let mut out = String::with_capacity(text.len());
    for (index, (line, &hidden)) in lines.iter().zip(&hide).enumerate() {
        if !hidden {
            out.push_str(line);
            continue;
        }
        if index == 0 || !hide[index - 1] {
            out.push_str(PLACEHOLDER);
        }
        if hide.get(index + 1) != Some(&true) && line.ends_with('\n') {
            out.push('\n');
        }
    }
    Some(out)
}

#[cfg(test)]
#[path = "secret_tests.rs"]
mod tests;
