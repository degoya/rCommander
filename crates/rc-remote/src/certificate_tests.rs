//! The certificates of the test CA (`tests/fixtures/certs`): read, described, checked against
//! their key and their validity.

use super::*;
use crate::testkit::{
    CLIENT_ED25519_CERT, CLIENT_ED25519_CERT_EXPIRED, CLIENT_ED25519_CERT_OTHER_PRINCIPAL,
    HOST_KEY_B_CERT, USER_CA, client_ed25519, user_ca,
};

/// A day in 2026 the fixtures' validity is judged on.
const NOW: u64 = 1_793_000_000;

#[test]
fn a_certificate_beside_its_key_is_described() {
    let info = inspect(CLIENT_ED25519_CERT, client_ed25519().public_key()).expect("taken");
    assert_eq!(info.cert_type, "ssh-ed25519-cert-v01@openssh.com");
    assert_eq!(info.key_id, "rc-test-valid");
    assert_eq!(info.serial, 1);
    assert_eq!(info.principals, ["tester"]);
    assert_eq!(info.valid_after, 0);
    assert_eq!(info.valid_before, None, "forever");
    assert_eq!(
        info.ca_fingerprint,
        user_ca().fingerprint(HashAlg::Sha256).to_string()
    );
    assert_eq!(info.openssh, CLIENT_ED25519_CERT.trim());
    assert_eq!(check_validity(&info, NOW), Ok(()));
    assert!(info.names("tester") && !info.names("root"));
}

#[test]
fn an_expired_certificate_is_taken_but_says_so() {
    let info = inspect(CLIENT_ED25519_CERT_EXPIRED, client_ed25519().public_key()).expect("taken");
    assert_eq!(info.key_id, "rc-test-expired");
    let before = info.valid_before.expect("an end");
    assert!(before < NOW, "ended in 2020");
    let error = check_validity(&info, NOW).expect_err("expired");
    assert_eq!(error.code(), SSH_CERTIFICATE_EXPIRED);
    assert_eq!(
        error,
        CertificateError::Expired {
            key_id: "rc-test-expired".to_owned(),
            valid_before: before,
        }
    );
    let early = check_validity(&info, info.valid_after - 1).expect_err("not yet");
    assert_eq!(early.code(), SSH_CERTIFICATE_NOT_YET_VALID);
}

#[test]
fn a_stored_keys_line_is_checked_like_the_key() {
    let line = crate::testkit::client_ed25519()
        .public_key()
        .to_openssh()
        .expect("encodes");
    let info = inspect_line(CLIENT_ED25519_CERT, &line).expect("taken");
    assert_eq!(info.key_id, "rc-test-valid");
    assert_eq!(
        inspect_line(CLIENT_ED25519_CERT, "ssh-ed25519 not-a-key"),
        Err(CertificateError::KeyMismatch)
    );
}

#[test]
fn another_keys_certificate_is_refused() {
    let error = inspect(HOST_KEY_B_CERT, client_ed25519().public_key()).expect_err("refused");
    assert_eq!(error, CertificateError::KeyMismatch);
    assert_eq!(error.code(), SSH_CERTIFICATE_KEY_MISMATCH);
}

#[test]
fn only_one_line_of_a_user_certificate_is_read() {
    let key = client_ed25519();
    let two = format!("{CLIENT_ED25519_CERT}{CLIENT_ED25519_CERT}");
    let huge = format!(
        "{} {}",
        CLIENT_ED25519_CERT.trim(),
        "A".repeat(MAX_CERTIFICATE_BYTES)
    );
    let damaged = CLIENT_ED25519_CERT.replacen("AAAA", "AAAB", 1);
    for text in [
        "",
        USER_CA,
        two.as_str(),
        huge.as_str(),
        damaged.as_str(),
        "ssh-ed25519-cert-v01@openssh.com not-base64",
    ] {
        assert_eq!(
            inspect(text, key.public_key()),
            Err(CertificateError::Invalid),
            "{text:.60}"
        );
    }
}

#[test]
fn a_refusal_for_the_principal_names_both_sides() {
    let info = inspect(
        CLIENT_ED25519_CERT_OTHER_PRINCIPAL,
        client_ed25519().public_key(),
    )
    .expect("taken");
    let error = CertificateError::Principal {
        key_id: info.key_id.clone(),
        user: "tester".to_owned(),
        principals: info.principals.clone(),
    };
    assert_eq!(error.code(), SSH_CERTIFICATE_PRINCIPAL);
    let message = error.to_string();
    assert!(
        message.contains("someone-else") && message.contains("tester"),
        "{message}"
    );
}

#[test]
fn every_code_is_listed_once() {
    let mut codes = CertificateError::ALL_CODES.to_vec();
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), CertificateError::ALL_CODES.len());
}
