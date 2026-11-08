use super::*;
use crate::pem;
use crate::report::Value;
use crate::test_support::fixture;

fn rsa_2048() -> Value {
    Value::Key {
        algorithm: "RSA".to_owned(),
        bits: Some(2048),
    }
}

fn der(name: &str) -> Vec<u8> {
    pem::blocks(fixture(name), 1)
        .into_iter()
        .next()
        .and_then(|block| block.body.ok())
        .expect("a block")
}

#[test]
fn a_leaf_certificate_as_openssl_shows_it() {
    let cert = certificate(&der("leaf.crt")).expect("a certificate");
    assert_eq!(cert.common_name.as_deref(), Some("example.test"));
    assert!(cert.subject.contains("CN=example.test"));
    assert!(cert.issuer.contains("CN=Test Root CA"));
    assert_eq!(cert.serial, "01:23:45:67:89:AB:CD:EF");
    assert_eq!(cert.version, 3);
    assert_eq!(cert.not_before, 1_767_225_600);
    assert_eq!(cert.not_after, 1_798_761_600);
    assert_eq!(
        cert.sha256,
        "0D:12:EC:E3:31:EE:20:CE:44:32:C6:85:69:1A:A9:9D:52:8B:FB:8F:C8:95:BD:6B:3E:AF:8F:E7:9C:D1:6A:12"
    );
    assert_eq!(
        cert.sha1,
        "13:41:D9:A8:AD:F2:B9:F8:5C:FC:BB:4E:8D:C4:4D:75:D4:1A:4F:0E"
    );
    assert_eq!(
        cert.extensions.alt_names,
        ["DNS:example.test", "DNS:www.example.test", "IP:192.0.2.1"]
    );
    assert_eq!(
        cert.extensions.key_usage.as_deref(),
        Some("digitalSignature, keyEncipherment")
    );
    assert_eq!(
        cert.extensions.extended_key_usage.as_deref(),
        Some("serverAuth, clientAuth")
    );
    assert_eq!(cert.extensions.basic_constraints, Some((false, None)));
    assert_eq!(
        cert.extensions
            .subject_key_id
            .as_deref()
            .map(show::hex)
            .as_deref(),
        Some("B4:46:33:8B:BE:F9:4D:AB:58:30:F7:60:D1:07:7E:DB:46:B8:4D:44")
    );
    assert_eq!(
        cert.extensions
            .authority_key_id
            .as_deref()
            .map(show::hex)
            .as_deref(),
        Some("F6:CA:CC:72:95:83:89:89:73:12:4F:EC:1E:E3:2F:2F:1D:A6:37:0B")
    );
    assert_eq!(cert.signature.name, "ecdsa-with-SHA256");
    assert_eq!(cert.key.as_ref().map(PublicKey::describe), Some(rsa_2048()));
    assert!(!cert.self_signed());
    assert!(!cert.is_ca());
}

#[test]
fn the_root_is_self_signed_and_a_ca() {
    let root = certificate(&der("root.crt")).expect("a certificate");
    assert!(root.self_signed());
    assert!(root.is_ca());
    assert_eq!(
        root.key.as_ref().map(PublicKey::algorithm).as_deref(),
        Some("EC P-256")
    );
}

#[test]
fn der_and_pem_are_the_same_certificate() {
    assert_eq!(
        certificate(fixture("leaf.der")),
        certificate(&der("leaf.crt"))
    );
}

#[test]
fn a_weak_certificate() {
    let weak = certificate(&der("weak.crt")).expect("a certificate");
    assert_eq!(weak.signature.strength, Strength::Weak);
    assert_eq!(weak.key.as_ref().and_then(PublicKey::bits), Some(1024));
}

#[test]
fn a_request() {
    let request = request(&der("leaf.csr")).expect("a request");
    assert_eq!(request.common_name.as_deref(), Some("example.test"));
    assert_eq!(request.signature.name, "sha256WithRSAEncryption");
    assert_eq!(request.extensions.alt_names.len(), 3);
    assert_eq!(
        request.key,
        certificate(&der("leaf.crt")).and_then(|cert| cert.key)
    );
}

#[test]
fn damaged_certificates_are_none_never_a_panic() {
    let bytes = der("leaf.crt");
    for end in 0..bytes.len() {
        assert_eq!(certificate(bytes.get(..end).unwrap_or_default()), None);
    }
    for index in 0..bytes.len() {
        let mut flipped = bytes.clone();
        if let Some(byte) = flipped.get_mut(index) {
            *byte ^= 0x5a;
        }
        let _ = certificate(&flipped);
        let _ = request(&flipped);
    }
}
