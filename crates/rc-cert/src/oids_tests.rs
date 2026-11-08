use super::*;

#[test]
fn dotted_forms() {
    assert_eq!(
        dotted(encoded::RSA).as_deref(),
        Some("1.2.840.113549.1.1.1")
    );
    assert_eq!(dotted(encoded::P384).as_deref(), Some("1.3.132.0.34"));
    assert_eq!(dotted(encoded::ED25519).as_deref(), Some("1.3.101.112"));
    // 2.999: the first byte carries 2 * 40 + 999 in two bytes.
    assert_eq!(dotted(&[0x88, 0x37]).as_deref(), Some("2.999"));
}

#[test]
fn malformed_identifiers_are_none() {
    assert_eq!(dotted(&[]), None);
    assert_eq!(dotted(&[0x2a, 0x86]), None);
    assert_eq!(dotted(&[0xff; 65]), None);
    assert_eq!(dotted(&[0xff; 12]), None);
}

#[test]
fn names_and_strengths() {
    assert_eq!(
        signature("1.2.840.113549.1.1.5"),
        ("sha1WithRSAEncryption".to_owned(), Strength::Weak)
    );
    assert_eq!(
        signature("1.2.840.113549.1.1.4"),
        ("md5WithRSAEncryption".to_owned(), Strength::Broken)
    );
    assert_eq!(signature("1.2.3"), ("1.2.3".to_owned(), Strength::Good));
    assert_eq!(curve(encoded::P256), Some(("P-256", 256)));
    assert_eq!(curve(encoded::RSA), None);
    assert_eq!(
        cipher(&[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x01, 0x2a]),
        "AES-256-CBC"
    );
    assert_eq!(cipher(&[0x2a, 0x03]), "1.2.3");
}
