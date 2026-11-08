use super::*;
use crate::object::Damage;
use crate::test_support::fixture;

#[test]
fn finds_every_block_of_a_bundle() {
    let blocks = blocks(fixture("chain-and-key.pem"), 100);
    let labels: Vec<&str> = blocks.iter().map(|block| block.label.as_str()).collect();
    assert_eq!(labels, ["CERTIFICATE", "CERTIFICATE", "PRIVATE KEY"]);
    assert_eq!(blocks.first().map(|block| block.line), Some(1));
    assert!(blocks.iter().all(|block| block.body.is_ok()));
}

#[test]
fn reads_legacy_headers() {
    let blocks = blocks(fixture("leaf-rsa-legacy-enc.key"), 10);
    let block = blocks.first().expect("one block");
    assert_eq!(block.label, "RSA PRIVATE KEY");
    assert_eq!(
        block.headers.first(),
        Some(&("Proc-Type".to_owned(), "4,ENCRYPTED".to_owned()))
    );
    assert!(
        block
            .headers
            .iter()
            .any(|(name, value)| name == "DEK-Info" && value.starts_with("AES-128-CBC,"))
    );
    assert!(block.body.is_ok());
}

#[test]
fn a_block_without_its_end_or_with_bad_base64_says_so() {
    let open = b"-----BEGIN CERTIFICATE-----\nMIIB\n";
    let blocks = blocks(open, 10);
    assert_eq!(
        blocks.first().map(|block| block.body.clone()),
        Some(Err(Damage::NoEnd))
    );

    let wrong_end = b"-----BEGIN CERTIFICATE-----\nMIIB\n-----END PRIVATE KEY-----\n";
    assert!(
        super::blocks(wrong_end, 10)
            .first()
            .is_some_and(|block| block.body.is_err())
    );

    let bad = b"-----BEGIN CERTIFICATE-----\nM*IB\n-----END CERTIFICATE-----\n";
    assert_eq!(
        super::blocks(bad, 10)
            .first()
            .map(|block| block.body.clone()),
        Some(Err(Damage::BadBase64))
    );
}

#[test]
fn crlf_and_text_around_blocks() {
    let text = b"subject=CN=x\r\n-----BEGIN PUBLIC KEY-----\r\nAAEC\r\n-----END PUBLIC KEY-----\r\ntrailer\r\n";
    let blocks = blocks(text, 10);
    assert_eq!(blocks.len(), 1);
    assert_eq!(blocks.first().map(|block| block.line), Some(2));
    assert_eq!(
        blocks.first().map(|block| block.body.clone()),
        Some(Ok(vec![0, 1, 2]))
    );
}

#[test]
fn known_labels_only() {
    assert!(has_known_block(fixture("leaf.crt")));
    assert!(has_known_block(fixture("id_ed25519")));
    assert!(!has_known_block(b"-----BEGIN PGP SIGNATURE-----\n"));
    assert!(!has_known_block(b"no armour here"));
}

#[test]
fn stops_at_the_limit() {
    let one = fixture("leaf.crt");
    let many: Vec<u8> = one.iter().copied().cycle().take(one.len() * 5).collect();
    assert_eq!(blocks(&many, 3).len(), 3);
}
