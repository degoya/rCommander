//! Reading an OpenSSH `known_hosts` file for the import into the trust store.
//!
//! Each name of a line becomes its own entry: a plain name on port 22, `[host]:port` on its
//! port, and a hashed name (`|1|salt|hash`) as it is - it is checked by HMAC-SHA1 when a key is
//! verified, never resolved. What the trust store cannot hold is skipped and reported: the
//! markers `@revoked` and `@cert-authority`, wildcard and negated patterns, and lines or keys
//! that do not parse. A key a `@revoked` line names is imported from no other line of the file
//! either (RC-080-12). The file itself is only read, never written.

use std::{io, path::Path, path::PathBuf};

use rc_db::{HASHED_PREFIX, KnownHostSource, NewKnownHost};
use serde::{Deserialize, Serialize};

use crate::{
    hashed,
    hostkey::{DEFAULT_PORT, OfferedKey, normalize_host},
};

/// Why a line or one name of it was not imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipReason {
    /// `@revoked`: a key marked as revoked is no key to trust.
    Revoked,
    /// `@cert-authority`: certificate trust is not supported.
    CertAuthority,
    /// Another `@` marker.
    UnknownMarker,
    /// A wildcard (`*`, `?`) or negated (`!`) pattern, which names no single host.
    Pattern,
    /// Too few fields, or a name that does not read (`[host]` without a port, a broken hash).
    Malformed,
    /// The key does not parse as a key of the type the line names.
    InvalidKey,
}

/// One line, or one name of a line, that was not imported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    /// The line number, from 1.
    pub line: usize,
    pub reason: SkipReason,
    /// The name skipped, when one name of a line was; `None` for the whole line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// A parsed file: the entries to store and what was skipped.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedKnownHosts {
    pub entries: Vec<NewKnownHost>,
    pub skipped: Vec<Skipped>,
}

/// The user's own `known_hosts`: `~/.ssh/known_hosts`, on Windows
/// `%USERPROFILE%\.ssh\known_hosts`. `None` when the platform reports no home directory.
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".ssh").join("known_hosts"))
}

/// Reads and parses the file at `path`.
///
/// # Errors
///
/// The file cannot be read.
pub async fn read(path: &Path) -> io::Result<ParsedKnownHosts> {
    let bytes = tokio::fs::read(path).await?;
    Ok(parse(&String::from_utf8_lossy(&bytes)))
}

/// Parses the text of a `known_hosts` file.
#[must_use]
pub fn parse(text: &str) -> ParsedKnownHosts {
    let mut parsed = ParsedKnownHosts::default();
    // The fingerprints of revoked keys, and the line of each entry.
    let mut revoked = Vec::new();
    let mut lines = Vec::new();
    for (index, line) in text.lines().enumerate() {
        parse_line(index + 1, line, &mut parsed, &mut revoked);
        lines.resize(parsed.entries.len(), index + 1);
    }
    // A revoked key is trusted from no line, whichever comes first.
    if !revoked.is_empty() {
        for (entry, line) in std::mem::take(&mut parsed.entries).into_iter().zip(lines) {
            if revoked.contains(&entry.fingerprint_sha256) {
                parsed.skipped.push(Skipped {
                    line,
                    reason: SkipReason::Revoked,
                    name: Some(entry.host),
                });
            } else {
                parsed.entries.push(entry);
            }
        }
    }
    parsed
}

fn parse_line(number: usize, line: &str, parsed: &mut ParsedKnownHosts, revoked: &mut Vec<String>) {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return;
    }
    let skip = |parsed: &mut ParsedKnownHosts, reason, name: Option<&str>| {
        parsed.skipped.push(Skipped {
            line: number,
            reason,
            name: name.map(str::to_owned),
        });
    };
    let mut fields = line.split_whitespace();
    let Some(first) = fields.next() else {
        return;
    };
    if let Some(marker) = first.strip_prefix('@') {
        // `@revoked <names> <type> <key>`: that key is refused from every line.
        if marker == "revoked"
            && let (Some(_), Some(algorithm), Some(base64)) =
                (fields.next(), fields.next(), fields.next())
            && let Ok(key) = OfferedKey::from_openssh(algorithm, base64)
        {
            revoked.push(key.fingerprint);
        }
        let reason = match marker {
            "revoked" => SkipReason::Revoked,
            "cert-authority" => SkipReason::CertAuthority,
            _ => SkipReason::UnknownMarker,
        };
        skip(parsed, reason, None);
        return;
    }
    let (Some(algorithm), Some(base64)) = (fields.next(), fields.next()) else {
        skip(parsed, SkipReason::Malformed, None);
        return;
    };
    let Ok(key) = OfferedKey::from_openssh(algorithm, base64) else {
        skip(parsed, SkipReason::InvalidKey, None);
        return;
    };
    for name in first.split(',').filter(|name| !name.is_empty()) {
        match host_and_port(name) {
            Ok((host, port)) => parsed.entries.push(NewKnownHost {
                host,
                port,
                algorithm: key.algorithm.clone(),
                public_key: key.public_key.clone(),
                fingerprint_sha256: key.fingerprint.clone(),
                source: KnownHostSource::Import,
            }),
            Err(reason) => skip(parsed, reason, Some(name)),
        }
    }
}

/// The host and port one name stands for; a hashed name keeps its hash and port 0.
fn host_and_port(name: &str) -> Result<(String, u16), SkipReason> {
    if name.starts_with(HASHED_PREFIX) {
        return if hashed::is_well_formed(name) {
            Ok((name.to_owned(), 0))
        } else {
            Err(SkipReason::Malformed)
        };
    }
    if name.starts_with('!') || name.contains(['*', '?']) {
        return Err(SkipReason::Pattern);
    }
    let (host, port) = match name.strip_prefix('[') {
        Some(bracketed) => {
            let (host, port) = bracketed.split_once("]:").ok_or(SkipReason::Malformed)?;
            (host, port.parse().map_err(|_| SkipReason::Malformed)?)
        }
        None => (name, DEFAULT_PORT),
    };
    let host = normalize_host(host);
    if host.is_empty() || port == 0 {
        return Err(SkipReason::Malformed);
    }
    Ok((host, port))
}

#[cfg(test)]
#[path = "known_hosts_import_tests.rs"]
mod tests;
