//! Certificates and keys taken apart and checked, for the built-in viewer (RC-110-08): X.509
//! certificates and requests, private and public keys in PEM, DER and OpenSSH form. [`read`]
//! gives a [`Report`]: the checks - validity against the caller's clock, the chain inside the
//! file, a key against its certificate, weak keys and signatures - and a section per object.
//! The report is data; rCommander's viewer puts it into words (`app/viewer/cert.rs`).
//!
//! - **Private keys stay private.** A private key is read only for its format, algorithm,
//!   encryption and public half; no private number and no PEM body is ever copied into the
//!   report, and an encrypted key is never decrypted (`privkey`).
//! - **Signatures are not verified.** The chain is linked by names and key identifiers; the
//!   checks say so in a line of their own.
//! - **Input from outside.** The caller hands in at most [`MAX_FILE_BYTES`], at most
//!   [`MAX_OBJECTS`] objects are reported, every value is cleaned and capped (`show`), and
//!   nothing panics on damaged bytes (`fuzz/fuzz_targets/cert_viewer.rs`).
//!
//! No I/O, no async, no GUI: rc-engine's `viewer::cert` reads the file and the setting.

pub mod cert;
pub mod checks;
pub mod der;
pub mod object;
pub mod oids;
pub mod pem;
pub mod privkey;
pub mod pubkey;
pub mod report;
pub mod show;
pub mod ssh;

#[cfg(test)]
mod test_support;

use checks::{Check, Clock, Finding, Level};
use report::Section;

/// The largest file read.
pub const MAX_FILE_BYTES: u64 = 4 << 20;
/// The most objects reported.
pub const MAX_OBJECTS: usize = 2000;
/// The bytes [`probe`] wants to see of a file's start (fewer for a smaller file).
pub const PROBE_BYTES: usize = 64 << 10;

/// The report of one file: the checks first, then a section per object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Report {
    pub checks: Vec<Check>,
    pub sections: Vec<Section>,
}

impl Report {
    /// The worst level among the checks.
    #[must_use]
    pub fn worst(&self) -> Level {
        self.checks
            .iter()
            .map(|check| check.level)
            .max()
            .unwrap_or(Level::Ok)
    }
}

/// Whether a file of `size` bytes whose start is `head` is one this reader takes: a PEM block of
/// a known label, one DER structure spanning the file, or an OpenSSH public key line.
#[must_use]
pub fn probe(head: &[u8], size: u64) -> bool {
    if size == 0 || size > MAX_FILE_BYTES {
        return false;
    }
    if pem::has_known_block(head) || object::has_key_line(head) {
        return true;
    }
    // A DER file is one SEQUENCE spanning exactly the file.
    head.first() == Some(&der::tag::SEQUENCE) && der_spans(head, size)
}

/// Whether the DER header at the front of `head` declares a SEQUENCE exactly `size` bytes long.
fn der_spans(head: &[u8], size: u64) -> bool {
    let Some(&first) = head.get(1) else {
        return false;
    };
    let (header, length) = if first < 0x80 {
        (2u64, u64::from(first))
    } else {
        let count = usize::from(first & 0x7f);
        if count == 0 || count > 4 {
            return false;
        }
        let Some(bytes) = head.get(2..2 + count) else {
            return false;
        };
        let length = bytes
            .iter()
            .fold(0u64, |length, &byte| (length << 8) | u64::from(byte));
        (2 + u64::from(first & 0x7f), length)
    };
    header.saturating_add(length) == size
}

/// The report of a whole file, checked at `clock`; `None` when it holds nothing this reader
/// knows or is larger than [`MAX_FILE_BYTES`].
#[must_use]
pub fn read(bytes: &[u8], clock: Clock) -> Option<Report> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return None;
    }
    let (objects, more) = object::parse(bytes, MAX_OBJECTS);
    if objects.is_empty() {
        return None;
    }
    let mut checks = checks::run(&objects, clock);
    if more {
        checks.push(Check {
            level: Level::Info,
            finding: Finding::MoreObjects {
                shown: objects.len(),
            },
        });
    }
    let sections = report::sections(&objects, clock);
    Some(Report { checks, sections })
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
