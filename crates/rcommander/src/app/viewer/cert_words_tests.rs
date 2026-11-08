//! A certificate report in words (RC-110-08): the checks first, every object's fields with their
//! labels, the text with its labels padded, and no private key material anywhere.

use rc_cert::{checks::Clock, read};

use super::*;
use crate::i18n::t;

const CHAIN_AND_KEY: &[u8] = include_bytes!("../../../../rc-cert/tests/fixtures/chain-and-key.pem");
const LEGACY_KEY: &[u8] =
    include_bytes!("../../../../rc-cert/tests/fixtures/leaf-rsa-legacy-enc.key");

/// 2026-06-01 00:00:00 UTC, warning 30 days ahead.
const CLOCK: Clock = Clock {
    now: 1_780_272_000,
    warn_days: 30,
};

fn words(bytes: &[u8]) -> Vec<Section> {
    sections(&read(bytes, CLOCK).expect("a report"))
}

#[test]
fn the_checks_come_first_then_each_object() {
    let sections = words(CHAIN_AND_KEY);
    let titles: Vec<&str> = sections
        .iter()
        .map(|section| section.title.as_str())
        .collect();
    let certificate = t!("viewer.cert.kind.certificate");
    assert_eq!(
        titles,
        [
            t!("viewer.cert.checks").to_owned(),
            format!("#1 {certificate}"),
            format!("#2 {certificate}"),
            format!("#3 {}", t!("viewer.cert.kind.private_key")),
        ]
    );
    let checks = sections.first().expect("checks");
    assert_eq!(checks.value.as_deref(), Some(t!("viewer.cert.level.info")));
    assert!(
        checks
            .lines
            .iter()
            .any(|line| line.label == t!("viewer.cert.level.ok")
                && line.value
                    == t!(
                        "viewer.cert.finding.key_belongs_certificate",
                        object = 3,
                        holder = 1
                    ))
    );
}

#[test]
fn values_are_put_into_words() {
    let sections = words(CHAIN_AND_KEY);
    let leaf = sections.get(1).expect("the leaf");
    assert_eq!(leaf.value.as_deref(), Some("example.test"));
    let line = |label: &str| {
        leaf.lines
            .iter()
            .find(|line| line.label == label)
            .map(|line| line.value.clone())
    };
    assert_eq!(
        line(t!("viewer.cert.field.found_at")),
        Some(t!("viewer.cert.origin.pem", block = 1, line = 1))
    );
    assert_eq!(
        line(t!("viewer.cert.field.not_after")).as_deref(),
        Some("2027-01-01 00:00:00 UTC")
    );
    assert_eq!(
        line(t!("viewer.cert.field.public_key")),
        Some(t!("viewer.cert.key_bits", algorithm = "RSA", bits = 2048))
    );
    assert_eq!(
        line(t!("viewer.cert.field.basic_constraints")).as_deref(),
        Some(t!("viewer.cert.not_ca"))
    );
}

#[test]
fn an_encrypted_key_says_so_and_nothing_more() {
    let sections = words(LEGACY_KEY);
    let key = sections.get(1).expect("the key");
    assert!(
        key.lines
            .iter()
            .any(|line| line.value == t!("viewer.cert.encrypted.yes", cipher = "AES-128-CBC"))
    );
    assert!(
        key.lines
            .iter()
            .any(|line| line.value == t!("viewer.cert.no_public.encrypted"))
    );
}

#[test]
fn the_text_pads_every_label_to_the_longest() {
    let sections = words(CHAIN_AND_KEY);
    let text = text(&sections);
    let width = sections
        .iter()
        .flat_map(|section| &section.lines)
        .map(|line| line.label.chars().count())
        .max()
        .expect("lines");
    assert!(text.starts_with(&format!("{} - ", t!("viewer.cert.checks"))));
    for line in text.lines().filter(|line| line.starts_with("  ")) {
        assert!(line.chars().count() > width + 2, "{line:?}");
    }
}

#[test]
fn no_private_key_material_in_the_words() {
    for bytes in [CHAIN_AND_KEY, LEGACY_KEY] {
        let text = text(&words(bytes));
        let file = String::from_utf8_lossy(bytes);
        for line in file
            .lines()
            .filter(|line| line.len() >= 16 && !line.starts_with("-----"))
        {
            assert!(!text.contains(line), "a line of the file is in the report");
        }
        assert!(!text.contains("BEGIN"));
    }
}
