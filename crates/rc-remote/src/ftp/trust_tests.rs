//! The verdict on a presented certificate against the stored one, and `describe_certificate` on
//! certificates made with rcgen.

use rc_db::TrustedCertificate;
use rcgen::{CertificateParams, DnType, KeyPair, date_time_ymd};

use super::{
    CertificateVerdict, PinnedCertificates, certificate_verdict, describe_certificate, unix_now,
};
use crate::ftp::{CertificateRejection, CertificateTrust, TrustDecision, fingerprint};

/// 2020-01-01 and 2030-01-01, 00:00 UTC.
const JAN_2020: i64 = 1_577_836_800;
const JAN_2030: i64 = 1_893_456_000;

fn presented(der: &[u8], system_verified: bool) -> CertificateRejection {
    CertificateRejection {
        host: "nas.example".to_owned(),
        port: 990,
        fingerprint: fingerprint(der),
        reason: if system_verified {
            String::new()
        } else {
            "invalid peer certificate: UnknownIssuer".to_owned()
        },
        system_verified,
        end_entity: der.to_vec(),
    }
}

fn stored(der: &[u8], not_after: i64) -> TrustedCertificate {
    TrustedCertificate {
        id: 1,
        host: "nas.example".to_owned(),
        port: 990,
        fingerprint_sha256: fingerprint(der),
        subject: "CN=nas.example".to_owned(),
        issuer: "CN=nas.example".to_owned(),
        not_after,
        added_at: "2026-10-18T00:00:00.000Z".to_owned(),
        last_seen_at: None,
    }
}

/// A self-signed certificate for `nas.example`, valid 2020 to 2030.
fn self_signed() -> Vec<u8> {
    let key = KeyPair::generate().expect("key");
    let mut params = CertificateParams::new(vec!["nas.example".to_owned()]).expect("params");
    params
        .distinguished_name
        .push(DnType::CommonName, "nas.example");
    params
        .distinguished_name
        .push(DnType::OrganizationName, "Home");
    params.not_before = date_time_ymd(2020, 1, 1);
    params.not_after = date_time_ymd(2030, 1, 1);
    params
        .self_signed(&key)
        .expect("certificate")
        .der()
        .to_vec()
}

#[test]
fn describe_reads_subject_issuer_and_validity() {
    let der = self_signed();
    let info = describe_certificate(&der).expect("a certificate");
    assert!(info.subject.contains("CN=nas.example"), "{info:?}");
    assert!(info.subject.contains("O=Home"), "{info:?}");
    assert_eq!(info.issuer, info.subject, "self-signed");
    assert_eq!((info.not_before, info.not_after), (JAN_2020, JAN_2030));
    assert_eq!(describe_certificate(b"not a certificate"), None);
}

#[test]
fn the_stored_certificate_is_trusted_whatever_the_roots_say() {
    let der = self_signed();
    let pinned = stored(&der, JAN_2030);
    assert_eq!(
        certificate_verdict(Some(&pinned), &presented(&der, false), JAN_2020),
        CertificateVerdict::Trusted
    );
}

#[test]
fn another_certificate_is_a_change_even_when_the_roots_vouch_for_it() {
    let (old, new) = (self_signed(), self_signed());
    let pinned = stored(&old, JAN_2030);
    for system_verified in [false, true] {
        assert_eq!(
            certificate_verdict(Some(&pinned), &presented(&new, system_verified), JAN_2020),
            CertificateVerdict::Changed {
                stored: pinned.clone()
            }
        );
    }
}

#[test]
fn a_stored_certificate_past_its_validity_is_expired() {
    let der = self_signed();
    let pinned = stored(&der, JAN_2020);
    assert_eq!(
        certificate_verdict(Some(&pinned), &presented(&der, false), JAN_2030),
        CertificateVerdict::Expired {
            stored: pinned.clone()
        }
    );
}

#[test]
fn with_nothing_stored_the_roots_decide() {
    let der = self_signed();
    assert_eq!(
        certificate_verdict(None, &presented(&der, true), JAN_2020),
        CertificateVerdict::Trusted
    );
    assert_eq!(
        certificate_verdict(None, &presented(&der, false), JAN_2020),
        CertificateVerdict::Unknown
    );
}

#[test]
fn accept_once_answers_unknown_and_expired_but_never_changed() {
    let (der, other) = (self_signed(), self_signed());
    let once = Some(fingerprint(&der));

    let unknown = PinnedCertificates::new(None, once.clone());
    assert_eq!(
        unknown.decide(&presented(&der, false)),
        TrustDecision::Trust
    );
    assert_eq!(
        unknown.decide(&presented(&other, false)),
        TrustDecision::Refuse,
        "only the fingerprint the person saw"
    );

    let expired = PinnedCertificates::new(Some(stored(&der, JAN_2020)), once.clone());
    assert!(unix_now() > JAN_2020);
    assert_eq!(
        expired.decide(&presented(&der, false)),
        TrustDecision::Trust
    );
    let without = PinnedCertificates::new(Some(stored(&der, JAN_2020)), None);
    assert_eq!(
        without.decide(&presented(&der, false)),
        TrustDecision::Refuse
    );

    let changed = PinnedCertificates::new(Some(stored(&other, JAN_2030 * 2)), once);
    assert_eq!(
        changed.decide(&presented(&der, true)),
        TrustDecision::Refuse
    );
}
