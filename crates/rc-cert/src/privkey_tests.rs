use super::*;
use crate::pem;
use crate::test_support::fixture;

/// The keys of a fixture's first block.
fn keys(name: &str) -> Vec<PrivateKeyInfo> {
    let block = pem::blocks(fixture(name), 10)
        .into_iter()
        .next()
        .expect("a block");
    let body = block.body.clone().expect("a body");
    from_pem(&block.label, &block.headers, &body).expect("a private key")
}

fn key(name: &str) -> PrivateKeyInfo {
    keys(name).into_iter().next().expect("one key")
}

/// The leaf's public key, from its certificate.
const LEAF_KEY: &str = "1C:08:35:35:71:3F:8F:EB:FB:E0:DD:85:C2:D5:22:8E:A0:E5:81:A7:BB:25:D4:F9:8C:E2:42:09:C3:4A:93:47";

#[test]
fn pkcs8_and_pkcs1_rsa_give_the_same_public_half() {
    let pkcs8 = key("leaf-rsa.key");
    assert_eq!(pkcs8.format, "PKCS#8");
    assert_eq!(pkcs8.algorithm.as_deref(), Some("RSA"));
    assert_eq!(pkcs8.encrypted, None);
    assert_eq!(
        pkcs8.public.as_ref().map(PublicKey::fingerprint).as_deref(),
        Some(LEAF_KEY)
    );

    let pkcs1 = key("leaf-rsa-pkcs1.key");
    assert_eq!(pkcs1.format, "PKCS#1");
    assert_eq!(pkcs1.public, pkcs8.public);
}

#[test]
fn ec_keys_carry_their_public_point() {
    let sec1 = key("root-ec.key");
    assert_eq!(sec1.format, "SEC1");
    assert_eq!(sec1.algorithm.as_deref(), Some("EC P-256"));
    let pkcs8 = key("root-ec-pkcs8.key");
    assert_eq!(pkcs8.format, "PKCS#8");
    assert!(sec1.public.is_some());
    assert_eq!(sec1.public, pkcs8.public);
    assert_eq!(
        sec1.public.as_ref().map(PublicKey::fingerprint).as_deref(),
        Some(
            "78:AF:45:91:E3:11:82:A3:8F:C9:B8:89:DC:95:88:08:22:DC:C3:93:B5:65:DA:CB:E4:8D:B6:21:AA:60:6B:97"
        )
    );
}

#[test]
fn ed25519_without_its_public_half_says_why() {
    let ed = key("ed25519.key");
    assert_eq!(ed.algorithm.as_deref(), Some("Ed25519"));
    assert_eq!(ed.public, None);
    assert_eq!(ed.no_public, Some(NOT_STORED));
}

#[test]
fn dsa_traditional() {
    let dsa = key("dsa.key");
    assert_eq!(dsa.algorithm.as_deref(), Some("DSA"));
    assert_eq!(dsa.public.as_ref().and_then(PublicKey::bits), Some(2048));
}

#[test]
fn encrypted_keys_are_named_not_decrypted() {
    let pkcs8 = key("leaf-rsa-enc.key");
    assert_eq!(pkcs8.format, "PKCS#8 EncryptedPrivateKeyInfo");
    assert_eq!(
        pkcs8.encrypted.as_deref(),
        Some("PBES2: PBKDF2, AES-256-CBC")
    );
    assert_eq!(pkcs8.public, None);
    assert_eq!(pkcs8.no_public, Some(ENCRYPTED));

    let legacy = key("leaf-rsa-legacy-enc.key");
    assert_eq!(legacy.encrypted.as_deref(), Some("AES-128-CBC"));
    assert_eq!(legacy.algorithm.as_deref(), Some("RSA"));
    assert_eq!(legacy.public, None);
}

#[test]
fn openssh_keys_keep_their_public_half_even_when_encrypted() {
    let plain = key("id_ed25519");
    assert_eq!(plain.format, "OpenSSH");
    assert_eq!(plain.encrypted, None);
    assert_eq!(
        plain.ssh_fingerprint.as_deref(),
        Some("SHA256:h4aqM5BLHTKRyRe0Wf93JkqavgnAcEW4PY9jbEUawWI")
    );

    let encrypted = key("id_rsa_enc");
    assert_eq!(encrypted.encrypted.as_deref(), Some("aes256-ctr, bcrypt"));
    assert_eq!(
        encrypted.public.as_ref().and_then(PublicKey::bits),
        Some(2048)
    );
}

#[test]
fn damaged_bodies_are_none_or_partial_never_a_panic() {
    for name in [
        "leaf-rsa.key",
        "root-ec.key",
        "leaf-rsa-enc.key",
        "dsa.key",
        "id_rsa_enc",
    ] {
        let block = pem::blocks(fixture(name), 1)
            .into_iter()
            .next()
            .expect("block");
        let body = block.body.clone().expect("body");
        for end in 0..body.len() {
            let _ = from_pem(
                &block.label,
                &block.headers,
                body.get(..end).unwrap_or_default(),
            );
        }
    }
    assert_eq!(from_pem("CERTIFICATE", &[], b""), None);
}
