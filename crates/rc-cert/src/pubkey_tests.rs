use super::*;
use crate::pem;
use crate::test_support::fixture;

fn pem_body(name: &str) -> Vec<u8> {
    pem::blocks(fixture(name), 10)
        .into_iter()
        .next()
        .and_then(|block| block.body.ok())
        .expect("a block")
}

#[test]
fn rsa_spki_is_canonical_and_fingerprinted_as_openssl_does() {
    let spki = pem_body("leaf-rsa-public.pem");
    let key = PublicKey::from_spki(&spki).expect("RSA");
    assert_eq!(key.to_spki(), spki);
    assert_eq!(
        key.describe(),
        Value::Key {
            algorithm: "RSA".to_owned(),
            bits: Some(2048)
        }
    );
    // `openssl x509 -pubkey | openssl pkey -pubin -outform DER | sha256sum`
    assert_eq!(
        key.fingerprint(),
        "1C:08:35:35:71:3F:8F:EB:FB:E0:DD:85:C2:D5:22:8E:A0:E5:81:A7:BB:25:D4:F9:8C:E2:42:09:C3:4A:93:47"
    );
    assert!(key.weakness().is_none());
}

#[test]
fn pkcs1_public_key_is_the_same_key() {
    let pkcs1 = PublicKey::rsa_from_pkcs1(&pem_body("leaf-rsa-public-pkcs1.pem")).expect("RSA");
    let spki = PublicKey::from_spki(&pem_body("leaf-rsa-public.pem")).expect("RSA");
    assert_eq!(pkcs1, spki);
}

#[test]
fn ed25519_spki() {
    let spki = pem_body("ed25519-public.pem");
    let key = PublicKey::from_spki(&spki).expect("Ed25519");
    assert_eq!(key.to_spki(), spki);
    assert_eq!(key.algorithm(), "Ed25519");
    assert_eq!(key.bits(), Some(256));
    assert_eq!(
        key.fingerprint(),
        "1C:3B:AE:DA:12:5A:3E:29:CB:BD:E1:00:32:5E:5E:6A:D8:AB:17:2B:C9:DC:17:09:AD:64:DD:D4:9C:66:4D:45"
    );
    assert!(
        key.ssh_fingerprint()
            .is_some_and(|print| print.starts_with("SHA256:"))
    );
}

#[test]
fn ssh_blob_round_trip_matches_ssh_keygen() {
    let line = std::str::from_utf8(fixture("id_rsa_enc.pub")).expect("text");
    let key = crate::ssh::key_line(line)
        .and_then(|line| line.key)
        .expect("RSA");
    assert_eq!(
        key.ssh_fingerprint().as_deref(),
        Some("SHA256:Ham/CDQ4uv2V2CZhJqxQoyZJUGraxXOkNDEcveu0U5A")
    );
}

#[test]
fn weak_keys() {
    let small = PublicKey::Rsa {
        n: vec![0xff; 128],
        e: vec![1, 0, 1],
    };
    assert_eq!(small.weakness().map(|(level, _)| level), Some(Level::Error));
    let dsa = PublicKey::Dsa {
        p: vec![0xff; 256],
        q: vec![1],
        g: vec![2],
        y: vec![3],
    };
    assert_eq!(dsa.weakness().map(|(level, _)| level), Some(Level::Warn));
    let p192 = PublicKey::Ec {
        curve: vec![0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x01],
        point: vec![4],
    };
    assert_eq!(p192.weakness().map(|(level, _)| level), Some(Level::Warn));
}

#[test]
fn damaged_spki_is_none() {
    let spki = pem_body("leaf-rsa-public.pem");
    for end in 0..spki.len() {
        let _ = PublicKey::from_spki(spki.get(..end).unwrap_or_default());
    }
    assert_eq!(PublicKey::from_spki(&[0x30, 0x00]), None);
}
