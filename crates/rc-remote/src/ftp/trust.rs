//! The stored decisions about FTPS certificates (RC-070-02), the counterpart of the SSH host-key
//! verdict (`crate::hostkey`).
//!
//! A person trusts one certificate per host and port; `rc-db` keeps it ([`TrustedCertificate`]).
//! [`certificate_verdict`] holds what a server presented against it: the stored certificate
//! again is trusted (self-signed, from an unknown CA or for another name alike) unless its
//! validity has ended; another certificate is a change, even one the system roots vouch for;
//! with nothing stored the system roots decide. [`PinnedCertificates`] is that verdict as the handshake's
//! [`CertificateTrust`], with the one certificate a person accepted for a single session.
//! [`describe_certificate`] reads what the dialog shows: subject, issuer, validity.

use std::time::{SystemTime, UNIX_EPOCH};

use rc_db::TrustedCertificate;
use serde::{Deserialize, Serialize};

use super::tls::{CertificateRejection, CertificateTrust, TrustDecision};

/// What a certificate says about itself, for a person to read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificateInfo {
    /// The subject's distinguished name (`CN=nas.example, O=...`).
    pub subject: String,
    pub issuer: String,
    /// The start and the end of its validity, Unix seconds.
    pub not_before: i64,
    pub not_after: i64,
}

/// The subject, issuer and validity of a DER certificate; `None` when it does not parse.
#[must_use]
pub fn describe_certificate(der: &[u8]) -> Option<CertificateInfo> {
    let (_, certificate) = x509_parser::parse_x509_certificate(der).ok()?;
    let validity = certificate.validity();
    Some(CertificateInfo {
        subject: certificate.subject().to_string(),
        issuer: certificate.issuer().to_string(),
        not_before: validity.not_before.timestamp(),
        not_after: validity.not_after.timestamp(),
    })
}

/// A presented certificate against the one stored for its host and port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertificateVerdict {
    /// The stored certificate, still valid; or, with nothing stored, one the roots vouch for.
    Trusted,
    /// Nothing is stored and the roots do not vouch for it: the first-contact question.
    Unknown,
    /// Another certificate is stored: a hard stop, whatever the roots say.
    Changed { stored: TrustedCertificate },
    /// The stored certificate, but its validity ended: a warning, never a silent yes.
    Expired { stored: TrustedCertificate },
}

/// The verdict on `presented` with `stored` the certificate trusted for its host and port, at
/// `now` (Unix seconds).
#[must_use]
pub fn certificate_verdict(
    stored: Option<&TrustedCertificate>,
    presented: &CertificateRejection,
    now: i64,
) -> CertificateVerdict {
    match stored {
        Some(stored) if stored.fingerprint_sha256 == presented.fingerprint => {
            if now > stored.not_after {
                CertificateVerdict::Expired {
                    stored: stored.clone(),
                }
            } else {
                CertificateVerdict::Trusted
            }
        }
        Some(stored) => CertificateVerdict::Changed {
            stored: stored.clone(),
        },
        None if presented.system_verified => CertificateVerdict::Trusted,
        None => CertificateVerdict::Unknown,
    }
}

/// Now, Unix seconds.
#[must_use]
pub fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_secs()).unwrap_or(i64::MAX)
        })
}

/// The trust of one FTP session: the certificate stored for its server, and the fingerprint a
/// person accepted once - for this session's connections only, stored nowhere.
#[derive(Debug, Clone, Default)]
pub struct PinnedCertificates {
    stored: Option<TrustedCertificate>,
    once: Option<String>,
}

impl PinnedCertificates {
    #[must_use]
    pub fn new(stored: Option<TrustedCertificate>, once: Option<String>) -> Self {
        Self { stored, once }
    }
}

impl CertificateTrust for PinnedCertificates {
    fn decide(&self, presented: &CertificateRejection) -> TrustDecision {
        match certificate_verdict(self.stored.as_ref(), presented, unix_now()) {
            CertificateVerdict::Trusted => TrustDecision::Trust,
            // "Accept once" answers an unknown or an expired certificate, never a changed one.
            CertificateVerdict::Unknown | CertificateVerdict::Expired { .. }
                if self.once.as_deref() == Some(presented.fingerprint.as_str()) =>
            {
                TrustDecision::Trust
            }
            CertificateVerdict::Unknown
            | CertificateVerdict::Expired { .. }
            | CertificateVerdict::Changed { .. } => TrustDecision::Refuse,
        }
    }
}

#[cfg(test)]
#[path = "trust_tests.rs"]
mod tests;
