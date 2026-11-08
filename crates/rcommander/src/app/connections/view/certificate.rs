//! The OpenSSH user certificate of a stored key in the connection form (RC-060-09): its key id
//! and principals, its validity, and - in the danger colour - that it expired or is not yet
//! valid, so the person learns it before connecting.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rc_db::{CertificateValidity, KeyCertificate};

use crate::{format, i18n::t};

/// The lines shown under the stored key for `certificate` at the Unix time `now`, each with
/// whether it is a warning.
pub(super) fn lines(certificate: &KeyCertificate, now: u64) -> Vec<(String, bool)> {
    let principals = if certificate.principals.is_empty() {
        t!("connections.ssh_certificate_any").to_owned()
    } else {
        certificate.principals.join(", ")
    };
    let mut lines = vec![
        (
            t!(
                "connections.ssh_certificate",
                key_id = certificate.key_id,
                principals = principals
            ),
            false,
        ),
        (validity(certificate), false),
    ];
    match certificate.validity_at(now) {
        CertificateValidity::Valid => {}
        CertificateValidity::Expired => {
            lines.push((t!("connections.ssh_certificate_expired").to_owned(), true));
        }
        CertificateValidity::NotYetValid => {
            lines.push((
                t!("connections.ssh_certificate_not_yet_valid").to_owned(),
                true,
            ));
        }
    }
    lines
}

/// The validity line: from and until, either of them open.
fn validity(certificate: &KeyCertificate) -> String {
    let from = (certificate.valid_after != 0).then(|| date(certificate.valid_after));
    match (from, certificate.valid_before.map(date)) {
        (Some(from), Some(until)) => t!(
            "connections.ssh_certificate_valid",
            from = from,
            until = until
        ),
        (Some(from), None) => t!("connections.ssh_certificate_valid_from", from = from),
        (None, Some(until)) => t!("connections.ssh_certificate_valid_until", until = until),
        (None, None) => t!("connections.ssh_certificate_valid_open").to_owned(),
    }
}

/// The Unix time now; 0 for a clock before 1970.
pub(super) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// `seconds` after 1970 as the system's locale writes a date; the number when it is out of range.
fn date(seconds: u64) -> String {
    UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .map_or_else(|| seconds.to_string(), |time| format::date(Some(time)))
}

#[cfg(test)]
#[path = "certificate_tests.rs"]
mod tests;
