use std::sync::{Arc, Mutex};

use rc_db::TrustedCertificate;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, date_time_ymd,
};
use rustls::{
    RootCertStore,
    client::{WebPkiServerVerifier, danger::ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};

use super::{
    CertificateRejection, CertificateTrust, RefuseUnknown, TlsSetup, TrustDecision, Verifier,
    fingerprint,
};
use crate::ftp::PinnedCertificates;

#[test]
fn fingerprints_are_colon_separated_sha256() {
    // SHA-256 of "abc" (FIPS 180-2, appendix B.1).
    assert_eq!(
        fingerprint(b"abc"),
        "BA:78:16:BF:8F:01:CF:EA:41:41:40:DE:5D:AE:22:23:B0:03:61:A3:96:17:7A:9C:B4:10:FF:61:F2:00:15:AD"
    );
}

#[test]
fn by_default_the_system_roots_decide() {
    let rejection = CertificateRejection::default();
    assert_eq!(RefuseUnknown.decide(&rejection), TrustDecision::Refuse);
    let vouched = CertificateRejection {
        system_verified: true,
        ..CertificateRejection::default()
    };
    assert_eq!(RefuseUnknown.decide(&vouched), TrustDecision::Trust);
}

#[test]
fn a_handshake_without_a_refusal_is_a_plain_tls_error() {
    let setup = TlsSetup::new("localhost", 21, Arc::new(RefuseUnknown)).expect("TLS setup");
    let error = setup.handshake_error("handshake eof".to_owned());
    assert_eq!(error.code(), "ftp_tls_failed");
}

#[test]
fn every_control_connection_gets_its_own_resumption_store() {
    let setup = TlsSetup::new("localhost", 21, Arc::new(RefuseUnknown)).expect("TLS setup");
    let (first, second) = (setup.session(), setup.session());
    assert!(
        !Arc::ptr_eq(first.config(), second.config()),
        "a configuration, and with it a resumption store, of its own"
    );
}

/// A test CA, standing in for the system roots, and the verifier over it.
struct Roots {
    issuer: Issuer<'static, KeyPair>,
    system: Arc<dyn ServerCertVerifier>,
}

fn roots() -> Roots {
    let key = KeyPair::generate().expect("CA key");
    let mut params = CertificateParams::new(Vec::<String>::new()).expect("CA params");
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    params
        .distinguished_name
        .push(DnType::CommonName, "Test CA");
    let ca = params.self_signed(&key).expect("CA certificate");
    let mut store = RootCertStore::empty();
    store.add(ca.der().clone()).expect("root");
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let system = WebPkiServerVerifier::builder_with_provider(Arc::new(store), provider)
        .build()
        .expect("verifier");
    Roots {
        issuer: Issuer::new(params, key),
        system,
    }
}

/// A certificate for `nas.example`, signed by `roots`' CA or self-signed.
fn leaf(roots: Option<&Roots>, expired: bool) -> CertificateDer<'static> {
    let key = KeyPair::generate().expect("key");
    let mut params = CertificateParams::new(vec!["nas.example".to_owned()]).expect("params");
    params
        .distinguished_name
        .push(DnType::CommonName, "nas.example");
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    if expired {
        params.not_before = date_time_ymd(2019, 1, 1);
        params.not_after = date_time_ymd(2020, 1, 1);
    }
    let certificate = match roots {
        Some(roots) => params.signed_by(&key, &roots.issuer),
        None => params.self_signed(&key),
    };
    certificate.expect("certificate").der().clone()
}

fn pinned(der: &CertificateDer<'_>, not_after: i64) -> TrustedCertificate {
    TrustedCertificate {
        id: 1,
        host: "nas.example".to_owned(),
        port: 990,
        fingerprint_sha256: fingerprint(der.as_ref()),
        subject: "CN=nas.example".to_owned(),
        issuer: String::new(),
        not_after,
        added_at: String::new(),
        last_seen_at: None,
    }
}

/// Runs the handshake's certificate check for `server_name`: whether it passed, and the
/// rejection the connection would report.
fn check(
    roots: &Roots,
    trust: Arc<dyn CertificateTrust>,
    der: &CertificateDer<'_>,
    server_name: &str,
) -> (bool, Option<CertificateRejection>) {
    let rejected = Arc::new(Mutex::new(None));
    let verifier = Verifier {
        system: Some(Arc::clone(&roots.system)),
        provider: Arc::new(rustls::crypto::aws_lc_rs::default_provider()),
        trust,
        host: "nas.example".to_owned(),
        port: 990,
        rejected: Arc::clone(&rejected),
    };
    let name = ServerName::try_from(server_name.to_owned()).expect("name");
    let passed = verifier
        .verify_server_cert(der, &[], &name, &[], UnixTime::now())
        .is_ok();
    let rejection = rejected.lock().expect("slot").take();
    (passed, rejection)
}

#[test]
fn a_certificate_the_roots_vouch_for_needs_no_decision() {
    let roots = roots();
    let der = leaf(Some(&roots), false);
    assert_eq!(
        check(&roots, Arc::new(RefuseUnknown), &der, "nas.example"),
        (true, None)
    );
    let nothing_stored = Arc::new(PinnedCertificates::new(None, None));
    assert!(check(&roots, nothing_stored, &der, "nas.example").0);
}

#[test]
fn a_self_signed_certificate_is_refused_until_it_is_pinned() {
    let roots = roots();
    let der = leaf(None, false);
    let (passed, rejection) = check(&roots, Arc::new(RefuseUnknown), &der, "nas.example");
    assert!(!passed);
    let rejection = rejection.expect("reported");
    assert_eq!(rejection.fingerprint, fingerprint(der.as_ref()));
    assert!(!rejection.system_verified);
    assert!(!rejection.reason.is_empty());

    let trust = Arc::new(PinnedCertificates::new(Some(pinned(&der, i64::MAX)), None));
    assert_eq!(check(&roots, trust, &der, "nas.example"), (true, None));
}

#[test]
fn a_name_mismatch_goes_to_the_hook_and_a_pin_covers_it() {
    let roots = roots();
    let der = leaf(Some(&roots), false);
    let (passed, rejection) = check(&roots, Arc::new(RefuseUnknown), &der, "other.example");
    assert!(!passed);
    assert!(!rejection.expect("reported").system_verified);
    let trust = Arc::new(PinnedCertificates::new(Some(pinned(&der, i64::MAX)), None));
    assert!(check(&roots, trust, &der, "other.example").0);
}

#[test]
fn a_changed_certificate_is_stopped_even_when_validly_signed() {
    let roots = roots();
    let (old, new) = (leaf(None, false), leaf(Some(&roots), false));
    let trust = Arc::new(PinnedCertificates::new(Some(pinned(&old, i64::MAX)), None));
    let (passed, rejection) = check(&roots, trust, &new, "nas.example");
    assert!(!passed, "a pin for another certificate stops a valid one");
    let rejection = rejection.expect("reported");
    assert!(rejection.system_verified);
    assert_eq!(rejection.fingerprint, fingerprint(new.as_ref()));
}

#[test]
fn an_expired_pinned_certificate_is_not_accepted_silently() {
    let roots = roots();
    let der = leaf(None, true);
    let info = crate::ftp::describe_certificate(der.as_ref()).expect("parsed");
    let stored = pinned(&der, info.not_after);
    let trust = Arc::new(PinnedCertificates::new(Some(stored.clone()), None));
    let (passed, rejection) = check(&roots, trust, &der, "nas.example");
    assert!(!passed);
    assert!(rejection.is_some());
    let once = Arc::new(PinnedCertificates::new(
        Some(stored),
        Some(fingerprint(der.as_ref())),
    ));
    assert!(
        check(&roots, once, &der, "nas.example").0,
        "accepted once, on purpose"
    );
}
