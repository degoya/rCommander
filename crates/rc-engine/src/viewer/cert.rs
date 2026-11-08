//! Certificates and keys in Lister and Quick View (RC-110-08): a file the built-in viewer opens
//! is read by `rc_cert` when its name or start says it may be a certificate or key, and
//! `viewer.certificates` is on. One path for every such file, whether `rc_mcp::sensitive_path`
//! keeps it from plugins or not: the core's own reader, never a plugin.
//!
//! - [`read_report`] reads at most `rc_cert::MAX_FILE_BYTES` once the start has passed
//!   `rc_cert::probe`, and checks at the system's clock with
//!   `viewer.cert_expiry_warning_days`. The report holds no private key material
//!   (`rc_cert::privkey`); an encrypted key is never decrypted.
//! - The window puts the report into words and shows the text from memory
//!   (`super::memory_text`); nothing is written to a disk.

use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use rc_cert::{MAX_FILE_BYTES, PROBE_BYTES, Report, checks::Clock};
use rc_db::Db;

use super::FileWindow;
use crate::settings::{
    CERT_EXPIRY_WARNING_DAYS_KEY, VIEWER_CERTIFICATES_KEY, stored_bool, stored_setting,
};

/// The extensions of certificate and key files, lower case.
const EXTENSIONS: &[&str] = &[
    "pem", "crt", "cer", "csr", "key", "pub", "der", "p12", "pfx",
];
/// OpenSSH's own file names, without regard to case.
const NAMES: &[&str] = &[
    "authorized_keys",
    "authorized_keys2",
    "id_rsa",
    "id_dsa",
    "id_ecdsa",
    "id_ed25519",
    "id_ecdsa_sk",
    "id_ed25519_sk",
];
/// What `viewer.cert_expiry_warning_days` holds until it is changed.
const DEFAULT_WARN_DAYS: i64 = 30;

/// Whether a file of this name whose first bytes are `head` may be a certificate or key: one of
/// the extensions or names above, or a start like `-----BEGIN` (its first four bytes are enough).
/// Reads nothing.
#[must_use]
pub fn is_candidate(path: &Path, head: &[u8]) -> bool {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let extension = Path::new(&name)
        .extension()
        .map(|extension| extension.to_string_lossy().into_owned())
        .unwrap_or_default();
    EXTENSIONS.contains(&extension.as_str())
        || NAMES.contains(&name.as_str())
        || head.get(..4) == Some(b"----".as_slice())
}

/// The report of `file` when it is a certificate or key file and `viewer.certificates` is on
/// (on without a database); `name` is the path the person opened, `head` the file's first
/// bytes, as the Lister read them. Any
/// failure is no report: the file opens as it is.
pub async fn read_report(
    file: &FileWindow,
    name: &Path,
    head: &[u8],
    db: Option<&Db>,
) -> Option<Report> {
    let size = file.size();
    if size == 0 || size > MAX_FILE_BYTES || !is_candidate(name, head) {
        return None;
    }
    if let Some(db) = db
        && !stored_bool(db, VIEWER_CERTIFICATES_KEY).await
    {
        return None;
    }
    let whole = usize::try_from(size).ok()?;
    let start = file.read_uncached(0, whole.min(PROBE_BYTES)).await.ok()?;
    if !rc_cert::probe(&start, size) {
        return None;
    }
    let bytes = if start.len() == whole {
        start
    } else {
        file.read_uncached(0, whole).await.ok()?
    };
    let clock = Clock {
        now: now(),
        warn_days: warn_days(db).await,
    };
    rc_cert::read(&bytes, clock)
}

/// The system's time in Unix seconds; 0 before 1970.
fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// `viewer.cert_expiry_warning_days`; its default without a database.
async fn warn_days(db: Option<&Db>) -> i64 {
    let Some(db) = db else {
        return DEFAULT_WARN_DAYS;
    };
    stored_setting(db, CERT_EXPIRY_WARNING_DAYS_KEY)
        .await
        .and_then(|value| value.as_i64())
        .unwrap_or(DEFAULT_WARN_DAYS)
}

#[cfg(test)]
#[path = "cert_tests.rs"]
mod tests;
