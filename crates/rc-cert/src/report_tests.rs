use super::*;
use crate::object::Origin;
use crate::test_support::{fixture, report};

fn fields(name: &str, section: usize) -> Vec<(Field, Value)> {
    report(fixture(name))
        .sections
        .into_iter()
        .nth(section)
        .map(|section| section.fields)
        .expect("a section")
}

fn has(fields: &[(Field, Value)], field: Field, value: Value) -> bool {
    fields.contains(&(field, value))
}

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

#[test]
fn a_certificate_section_has_every_field() {
    let fields = fields("leaf.crt", 0);
    let order: Vec<Field> = fields.iter().map(|(field, _)| *field).collect();
    assert_eq!(
        order,
        [
            Field::FoundAt,
            Field::Subject,
            Field::Issuer,
            Field::Serial,
            Field::Version,
            Field::NotBefore,
            Field::NotAfter,
            Field::DaysLeft,
            Field::AltNames,
            Field::KeyUsage,
            Field::ExtendedKeyUsage,
            Field::BasicConstraints,
            Field::PublicKey,
            Field::PublicKeySha256,
            Field::SshFingerprint,
            Field::SignatureAlgorithm,
            Field::Sha1Fingerprint,
            Field::Sha256Fingerprint,
            Field::SubjectKeyId,
            Field::AuthorityKeyId,
        ]
    );
    assert!(has(
        &fields,
        Field::FoundAt,
        Value::Origin(Origin::Pem { block: 1, line: 1 })
    ));
    assert!(has(&fields, Field::Serial, text("01:23:45:67:89:AB:CD:EF")));
    assert!(has(&fields, Field::NotAfter, Value::Date(1_798_761_600)));
    assert!(has(&fields, Field::DaysLeft, Value::Number(214)));
    assert!(has(
        &fields,
        Field::AltNames,
        text("DNS:example.test, DNS:www.example.test, IP:192.0.2.1")
    ));
    assert!(has(
        &fields,
        Field::BasicConstraints,
        Value::BasicConstraints {
            ca: false,
            path_length: None
        }
    ));
    assert!(has(
        &fields,
        Field::PublicKey,
        Value::Key {
            algorithm: "RSA".to_owned(),
            bits: Some(2048)
        }
    ));
}

#[test]
fn a_request_section() {
    let fields = fields("leaf.csr", 0);
    assert!(fields.iter().any(|(field, _)| *field == Field::AltNames));
    assert!(has(
        &fields,
        Field::SignatureAlgorithm,
        text("sha256WithRSAEncryption")
    ));
}

#[test]
fn a_private_key_section_names_only_the_public_half() {
    let fields = fields("leaf-rsa-legacy-enc.key", 0);
    assert!(has(
        &fields,
        Field::Encrypted,
        Value::Encrypted(Some("AES-128-CBC".to_owned()))
    ));
    assert!(has(
        &fields,
        Field::PublicKey,
        Value::NoPublic(NoPublic::Encrypted)
    ));
    let plain = self::fields("leaf-rsa.key", 0);
    assert!(has(&plain, Field::Encrypted, Value::Encrypted(None)));
    assert!(
        plain
            .iter()
            .any(|(field, _)| *field == Field::PublicKeySha256)
    );
}

#[test]
fn ssh_key_lines() {
    let fields = fields("authorized_keys", 1);
    assert!(has(&fields, Field::FoundAt, Value::Origin(Origin::Line(4))));
    assert!(has(
        &fields,
        Field::Options,
        text("command=\"/usr/bin/true\",no-pty,from=\"192.0.2.0/24\"")
    ));
    assert!(has(&fields, Field::Comment, text("ecdsa@example")));
}
