use super::*;
use crate::test_support::fixture;

fn kinds(name: &str) -> Vec<&'static str> {
    parse(fixture(name), 100)
        .0
        .iter()
        .map(|object| match object.body {
            Body::Certificate(_) => "certificate",
            Body::Request(_) => "request",
            Body::PrivateKey(_) => "private key",
            Body::PublicKey(_) => "public key",
            Body::SshKey(_) => "ssh key",
            Body::Unsupported(_) => "unsupported",
            Body::Damaged(_) => "damaged",
        })
        .collect()
}

#[test]
fn every_fixture_is_taken_apart() {
    assert_eq!(
        kinds("chain-and-key.pem"),
        ["certificate", "certificate", "private key"]
    );
    assert_eq!(kinds("leaf.der"), ["certificate"]);
    assert_eq!(kinds("leaf.csr"), ["request"]);
    assert_eq!(kinds("leaf-rsa-public.pem"), ["public key"]);
    assert_eq!(kinds("leaf-rsa-public-pkcs1.pem"), ["public key"]);
    assert_eq!(kinds("id_ed25519"), ["private key"]);
    assert_eq!(kinds("id_ed25519.pub"), ["ssh key"]);
    assert_eq!(kinds("leaf.p12"), ["unsupported"]);
    // Two keys, one with options, one more, and a broken line; the comment is skipped.
    assert_eq!(
        kinds("authorized_keys"),
        ["ssh key", "ssh key", "ssh key", "damaged"]
    );
}

#[test]
fn origins_name_block_and_line() {
    let (objects, more) = parse(fixture("fullchain.pem"), 100);
    assert!(!more);
    let origins: Vec<Origin> = objects.iter().map(|object| object.origin).collect();
    assert_eq!(origins.first(), Some(&Origin::Pem { block: 1, line: 1 }));
    assert!(matches!(origins.get(1), Some(Origin::Pem { block: 2, .. })));
    let (lines, _) = parse(fixture("authorized_keys"), 100);
    assert_eq!(
        lines.first().map(|object| object.origin),
        Some(Origin::Line(3))
    );
}

#[test]
fn the_limit_is_kept_and_reported() {
    let (objects, more) = parse(fixture("chain-and-key.pem"), 2);
    assert_eq!(objects.len(), 2);
    assert!(more);
    let (lines, more) = parse(fixture("authorized_keys"), 1);
    assert_eq!(lines.len(), 1);
    assert!(more);
}

#[test]
fn unknown_content_is_nothing() {
    assert_eq!(parse(b"hello\nworld\n", 100).0, Vec::new());
    assert_eq!(parse(b"", 100).0, Vec::new());
    assert!(!is_der(b"\x30\x05\x02\x01"));
    assert!(is_der(&[0x30, 0x03, 0x02, 0x01, 0x00]));
    assert_eq!(kinds_of(&[0x30, 0x03, 0x02, 0x01, 0x00]), [true]);
}

fn kinds_of(bytes: &[u8]) -> Vec<bool> {
    parse(bytes, 10)
        .0
        .iter()
        .map(|object| matches!(object.body, Body::Damaged(_)))
        .collect()
}

#[test]
fn unsupported_blocks_are_named() {
    let crl = b"-----BEGIN X509 CRL-----\nAAEC\n-----END X509 CRL-----\n";
    let (objects, _) = parse(crl, 10);
    assert!(matches!(
        &objects.first().map(|object| &object.body),
        Some(Body::Unsupported(Unsupported::Crl))
    ));
}
