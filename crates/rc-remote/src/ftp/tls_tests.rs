use std::sync::{Arc, Mutex};

use rc_db::TrustedCertificate;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose, date_time_ymd,
};
use rustls::{
    RootCertStore,
    client::{WebPkiServerVerifier, danger::ServerCertVerifier},
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
};

use super::{
    CertificateRejection, CertificateTrust, FtpError, HttpsTls, RefuseUnknown, TlsSession,
    TlsSetup, TrustDecision, Verifier, VerifierParts, fingerprint,
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
    let error = setup.session().handshake_error("handshake eof".to_owned());
    assert_eq!(error.code(), "ftp_tls_failed");
}

#[test]
fn every_control_connection_gets_its_own_resumption_store() {
    let setup = TlsSetup::new("localhost", 21, Arc::new(RefuseUnknown)).expect("TLS setup");
    let (first, second) = (setup.session(), setup.session());
    assert!(
        !Arc::ptr_eq(first.connector.config(), second.connector.config()),
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

/// The certificate check of one handshake of `session`, by a verifier as its connector holds:
/// the setup's parts over the session's refusal slot.
fn handshake(setup: &TlsSetup, session: &TlsSession, der: &CertificateDer<'_>) -> bool {
    let verifier = setup.parts.verifier(Arc::clone(&session.rejected));
    let name = ServerName::try_from("nas.example".to_owned()).expect("name");
    verifier
        .verify_server_cert(der, &[], &name, &[], UnixTime::now())
        .is_ok()
}

/// The fingerprint a failed handshake of `session` reports; `None` for a plain TLS error.
fn reported(session: &TlsSession) -> Option<String> {
    match session.handshake_error("handshake eof".to_owned()) {
        FtpError::CertificateUnknown(rejection) => Some(rejection.fingerprint),
        _ => None,
    }
}

/// RC-080-67: a refusal is reported for the handshake it happened in - never for another
/// control connection's, nor for a later handshake of the same one.
#[test]
fn a_refusal_reported_is_always_the_current_handshakes() {
    let roots = roots();
    let setup = TlsSetup::with_parts(VerifierParts {
        system: Some(Arc::clone(&roots.system)),
        provider: Arc::new(rustls::crypto::aws_lc_rs::default_provider()),
        trust: Arc::new(RefuseUnknown),
        host: "nas.example".to_owned(),
        port: 990,
    })
    .expect("TLS setup");
    let (old, new) = (leaf(None, false), leaf(None, false));
    let control = setup.session();
    // A data connection of `control` is refused `old`; nobody reads that refusal.
    assert!(!handshake(&setup, &control, &old));

    // Another control connection whose handshake fails otherwise reports no certificate.
    let other = setup.session();
    assert_eq!(reported(&other), None);

    // The next handshake of `control` starts empty, and reports its own certificate.
    control.begin_handshake();
    assert_eq!(reported(&control), None, "the earlier refusal is gone");
    assert!(!handshake(&setup, &control, &old));
    control.begin_handshake();
    assert!(!handshake(&setup, &control, &new));
    assert_eq!(reported(&control), Some(fingerprint(new.as_ref())));
    // A certificate the roots vouch for leaves the slot empty.
    control.begin_handshake();
    assert!(handshake(&setup, &control, &leaf(Some(&roots), false)));
    assert_eq!(reported(&control), None);
}

/// A self-signed certificate for `nas.example` and its key, as a Proxmox node makes its own.
fn self_signed_with_key() -> (CertificateDer<'static>, PrivateKeyDer<'static>) {
    let key = KeyPair::generate().expect("key");
    let mut params = CertificateParams::new(vec!["nas.example".to_owned()]).expect("params");
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let der = params.self_signed(&key).expect("certificate").der().clone();
    let key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(key.serialize_der()));
    (der, key)
}

/// A handshake in memory between a client over `config` and a server presenting `der`: whether
/// the client accepted it.
fn handshake_in_memory(
    config: rustls::ClientConfig,
    der: CertificateDer<'static>,
    key: PrivateKeyDer<'static>,
) -> bool {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let server_config = rustls::ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .expect("versions")
        .with_no_client_auth()
        .with_single_cert(vec![der], key)
        .expect("server configuration");
    let mut server = rustls::ServerConnection::new(Arc::new(server_config)).expect("server");
    let name = ServerName::try_from("nas.example".to_owned()).expect("name");
    let mut client = rustls::ClientConnection::new(Arc::new(config), name).expect("client");
    for _ in 0..8 {
        let mut bytes = Vec::new();
        client.write_tls(&mut bytes).expect("client writes");
        if !bytes.is_empty() {
            server
                .read_tls(&mut bytes.as_slice())
                .expect("server reads");
            if server.process_new_packets().is_err() {
                return false;
            }
        }
        let mut bytes = Vec::new();
        server.write_tls(&mut bytes).expect("server writes");
        if !bytes.is_empty() {
            client
                .read_tls(&mut bytes.as_slice())
                .expect("client reads");
            if client.process_new_packets().is_err() {
                return false;
            }
        }
        if !client.is_handshaking() {
            return true;
        }
    }
    false
}

/// RC-100-40: an inventory's HTTPS client refuses a self-signed certificate and names it, and
/// takes it once the person trusted it - the FTPS trust, never verification turned off.
#[test]
fn an_https_client_takes_a_self_signed_certificate_only_when_it_is_pinned() {
    let (der, key) = self_signed_with_key();
    let refusing = HttpsTls::new("nas.example", 8006, Arc::new(RefuseUnknown)).expect("TLS");
    assert_eq!(refusing.rejection(), None);
    assert!(!handshake_in_memory(
        refusing.client_config(),
        der.clone(),
        key.clone_key()
    ));
    let rejection = refusing
        .rejection()
        .expect("the refused certificate is named");
    assert_eq!(rejection.fingerprint, fingerprint(der.as_ref()));
    assert_eq!(
        (rejection.host.as_str(), rejection.port),
        ("nas.example", 8006)
    );
    assert!(!rejection.system_verified);

    let trust = Arc::new(PinnedCertificates::new(Some(pinned(&der, i64::MAX)), None));
    let pinned = HttpsTls::new("nas.example", 8006, trust).expect("TLS");
    assert!(handshake_in_memory(pinned.client_config(), der, key));
    assert_eq!(pinned.rejection(), None);
}
