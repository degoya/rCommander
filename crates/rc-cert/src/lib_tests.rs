use super::*;
use crate::object::{Damage, Unsupported};
use crate::report::{Field, Kind, Value};
use crate::test_support::{FIXTURES, PRIVATE_FIXTURES, clock, fixture, report};

fn head(bytes: &[u8]) -> &[u8] {
    bytes
        .get(..PROBE_BYTES.min(bytes.len()))
        .unwrap_or_default()
}

#[test]
fn probe_takes_every_fixture() {
    for (name, bytes) in FIXTURES {
        assert!(probe(head(bytes), bytes.len() as u64), "{name}");
    }
}

#[test]
fn probe_declines_what_is_no_certificate_or_key() {
    for bytes in [
        &b""[..],
        b"hello world\n",
        b"PK\x03\x04\x14\x00\x00\x00\x08\x00",
        b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1\x00\x00",
        b"-----BEGIN PGP SIGNATURE-----\nAAAA\n-----END PGP SIGNATURE-----\n",
        b"\x30\x82\x10\x00\x02\x01\x00",
    ] {
        assert!(!probe(bytes, bytes.len() as u64), "{bytes:?}");
    }
    assert!(!probe(fixture("leaf.crt"), MAX_FILE_BYTES + 1));
}

#[test]
fn nothing_known_is_no_report() {
    assert_eq!(read(b"just some text\n", clock()), None);
    assert_eq!(read(b"", clock()), None);
    let large = vec![b' '; usize::try_from(MAX_FILE_BYTES).unwrap_or(0) + 1];
    assert_eq!(read(&large, clock()), None);
}

#[test]
fn a_chain_with_its_key() {
    let report = report(fixture("chain-and-key.pem"));
    let kinds: Vec<Kind> = report.sections.iter().map(|section| section.kind).collect();
    assert_eq!(
        kinds,
        [Kind::Certificate, Kind::Certificate, Kind::PrivateKey]
    );
    assert_eq!(report.worst(), Level::Info);
    assert_eq!(
        report
            .sections
            .first()
            .and_then(|section| section.value.clone()),
        Some(Value::Text("example.test".to_owned()))
    );
}

#[test]
fn pkcs12_and_damage_are_sections_of_their_own() {
    let p12 = report(fixture("leaf.p12"));
    assert_eq!(
        p12.sections
            .first()
            .and_then(|section| section.value.clone()),
        Some(Value::Unsupported(Unsupported::Pkcs12))
    );
    let keys = report(fixture("authorized_keys"));
    assert_eq!(
        keys.sections
            .last()
            .and_then(|section| section.value.clone()),
        Some(Value::Damaged(Damage::NotAKeyLine))
    );
}

/// Everything a report holds as text, for the checks below.
fn shown(report: &Report) -> String {
    let mut shown = format!("{:?}", report.checks);
    for section in &report.sections {
        shown.push_str(&format!("{:?}", section.value));
        for (field, value) in &section.fields {
            shown.push_str(&format!("{field:?}"));
            if let Value::Text(text) = value {
                shown.push_str(text);
            } else {
                shown.push_str(&format!("{value:?}"));
            }
        }
    }
    shown
}

#[test]
fn private_key_material_never_reaches_the_report() {
    for name in PRIVATE_FIXTURES {
        let bytes = fixture(name);
        let shown = shown(&report(bytes));
        for block in crate::pem::blocks(bytes, 10) {
            if !block.label.contains("PRIVATE KEY") {
                continue;
            }
            let text = String::from_utf8_lossy(bytes);
            for line in text
                .lines()
                .filter(|line| line.len() >= 16 && !line.starts_with("-----"))
            {
                assert!(
                    !shown.contains(line),
                    "{name}: a line of the key body is in the report"
                );
            }
            let body = block.body.expect("body");
            for window in body.windows(8) {
                assert!(
                    !shown.contains(&show::hex(window)),
                    "{name}: key bytes as hex"
                );
            }
        }
        assert!(!shown.contains("BEGIN"), "{name}");
    }
}

#[test]
fn damaged_input_never_panics() {
    for (_, bytes) in FIXTURES {
        let step = (bytes.len() / 300).max(1);
        for end in (0..bytes.len()).step_by(step) {
            let prefix = bytes.get(..end).unwrap_or_default();
            let _ = probe(prefix, prefix.len() as u64);
            let _ = read(prefix, clock());
        }
        for index in (0..bytes.len()).step_by(step) {
            let mut flipped = bytes.to_vec();
            if let Some(byte) = flipped.get_mut(index) {
                *byte = byte.wrapping_add(0x41);
            }
            let _ = read(&flipped, clock());
        }
    }
    let mut noise = 0x9e37_79b9_u32;
    let random: Vec<u8> = (0..4096)
        .map(|_| {
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            noise.to_le_bytes()[0]
        })
        .collect();
    let _ = read(&random, clock());
    let mut der = vec![0x30, 0x82, 0x0f, 0xfc];
    der.extend_from_slice(&random[..4092]);
    let _ = read(&der, clock());
}

#[test]
fn many_objects_are_capped() {
    let one = fixture("leaf.crt");
    let many: Vec<u8> = one
        .iter()
        .copied()
        .cycle()
        .take(one.len() * (MAX_OBJECTS + 1))
        .collect();
    let report = report(&many);
    assert_eq!(report.sections.len(), MAX_OBJECTS);
    assert!(
        report
            .checks
            .iter()
            .any(|check| check.finding == Finding::MoreObjects { shown: MAX_OBJECTS })
    );
}

#[test]
fn pem_and_der_give_the_same_fields() {
    let fields = |name: &str| -> Vec<(Field, Value)> {
        report(fixture(name))
            .sections
            .into_iter()
            .flat_map(|section| section.fields)
            .filter(|(field, _)| *field != Field::FoundAt)
            .collect()
    };
    assert_eq!(fields("leaf.crt"), fields("leaf.der"));
}
