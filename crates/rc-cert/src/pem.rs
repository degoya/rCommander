//! PEM blocks (RFC 7468, with RFC 1421's headers that OpenSSL's legacy encrypted keys carry):
//! `-----BEGIN <label>-----`, optional `Name: value` lines and a blank line, Base64, and
//! `-----END <label>-----`. Read line by line from bytes that need not be UTF-8; text around the
//! blocks is ignored.

use base64::Engine as _;

use crate::object::Damage;
use base64::engine::GeneralPurpose;
use base64::engine::general_purpose::PAD_INDIFFERENT;

/// Base64 with or without its padding, as PEM writers differ.
pub(crate) const BASE64: GeneralPurpose =
    GeneralPurpose::new(&base64::alphabet::STANDARD, PAD_INDIFFERENT);

/// The most header lines a block may carry.
const MAX_HEADERS: usize = 16;
/// The longest label read.
const MAX_LABEL: usize = 64;

/// One block.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// The label of the `BEGIN` line (`CERTIFICATE`, `PRIVATE KEY`, ...).
    pub label: String,
    /// The line of the `BEGIN` line, from 1.
    pub line: usize,
    /// `Name: value` headers (`Proc-Type`, `DEK-Info`).
    pub headers: Vec<(String, String)>,
    /// The decoded body, or why it could not be read.
    pub body: Result<Vec<u8>, Damage>,
}

/// The labels a block of which may be shown, or named as not shown.
pub const KNOWN_LABELS: &[&str] = &[
    "CERTIFICATE",
    "TRUSTED CERTIFICATE",
    "X509 CERTIFICATE",
    "CERTIFICATE REQUEST",
    "NEW CERTIFICATE REQUEST",
    "PRIVATE KEY",
    "ENCRYPTED PRIVATE KEY",
    "RSA PRIVATE KEY",
    "EC PRIVATE KEY",
    "DSA PRIVATE KEY",
    "OPENSSH PRIVATE KEY",
    "PUBLIC KEY",
    "RSA PUBLIC KEY",
    "X509 CRL",
    "PKCS7",
    "CMS",
    "EC PARAMETERS",
    "DH PARAMETERS",
];

/// The label of a `-----BEGIN <label>-----` line.
#[must_use]
pub fn begin_label(line: &[u8]) -> Option<&str> {
    armour(line, b"-----BEGIN ")
}

fn armour<'a>(line: &'a [u8], prefix: &[u8]) -> Option<&'a str> {
    let label = trim(line).strip_prefix(prefix)?.strip_suffix(b"-----")?;
    if label.is_empty() || label.len() > MAX_LABEL {
        return None;
    }
    std::str::from_utf8(label).ok()
}

/// Whether the bytes hold a `BEGIN` line of a known label.
#[must_use]
pub fn has_known_block(bytes: &[u8]) -> bool {
    bytes
        .split(|&byte| byte == b'\n')
        .filter_map(begin_label)
        .any(|label| KNOWN_LABELS.contains(&label))
}

/// Every block, at most `max_blocks` of them; a block without its `END` line is one whose body
/// says so.
#[must_use]
pub fn blocks(bytes: &[u8], max_blocks: usize) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut lines = bytes.split(|&byte| byte == b'\n').enumerate();
    while let Some((index, line)) = lines.next() {
        if blocks.len() >= max_blocks {
            break;
        }
        let Some(label) = begin_label(line) else {
            continue;
        };
        let mut headers = Vec::new();
        let mut base64 = Vec::new();
        let mut in_headers = true;
        let mut ended = false;
        for (_, line) in lines.by_ref() {
            let line = trim(line);
            if let Some(end) = armour(line, b"-----END ") {
                ended = end == label;
                break;
            }
            if in_headers {
                if let Some(colon) = line.iter().position(|&byte| byte == b':') {
                    if headers.len() < MAX_HEADERS {
                        let (name, value) = line.split_at(colon);
                        headers.push((
                            crate::show::clean_bytes(trim(name)),
                            crate::show::clean_bytes(trim(value.get(1..).unwrap_or_default())),
                        ));
                    }
                    continue;
                }
                // The blank line after the headers, or the first line of Base64.
                in_headers = false;
            }
            base64.extend(line.iter().filter(|byte| !byte.is_ascii_whitespace()));
        }
        let body = if !ended {
            Err(Damage::NoEnd)
        } else {
            BASE64.decode(base64).map_err(|_| Damage::BadBase64)
        };
        blocks.push(Block {
            label: label.to_owned(),
            line: index + 1,
            headers,
            body,
        });
    }
    blocks
}

/// A line without the white space around it (and without a carriage return).
fn trim(line: &[u8]) -> &[u8] {
    line.trim_ascii()
}

#[cfg(test)]
#[path = "pem_tests.rs"]
mod tests;
