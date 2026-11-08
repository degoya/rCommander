use super::*;
use crate::cert::{Extensions, Signature};
use crate::object::{Origin, parse};
use crate::test_support::{MID_2026, clock, fixture};

fn checks(name: &str, clock: Clock) -> Vec<Check> {
    let (objects, _) = parse(fixture(name), 100);
    run(&objects, clock)
}

fn has(checks: &[Check], level: Level, finding: &Finding) -> bool {
    checks
        .iter()
        .any(|check| check.level == level && check.finding == *finding)
}

fn at(now: i64) -> Clock {
    Clock { now, ..clock() }
}

/// The leaf's end: 2027-01-01 00:00:00 UTC.
const LEAF_END: i64 = 1_798_761_600;
/// The leaf's start: 2026-01-01 00:00:00 UTC.
const LEAF_START: i64 = 1_767_225_600;

#[test]
fn a_chain_in_order() {
    let checks = checks("fullchain.pem", clock());
    let valid = Finding::ValidUntil {
        object: 1,
        until: LEAF_END,
        days: 214,
    };
    assert!(has(&checks, Level::Ok, &valid));
    assert!(has(
        &checks,
        Level::Ok,
        &Finding::IssuedBy {
            object: 1,
            issuer: 2
        }
    ));
    assert!(has(
        &checks,
        Level::Ok,
        &Finding::SelfSignedRoot { object: 2 }
    ));
    assert!(has(&checks, Level::Info, &Finding::SignaturesNotVerified));
    assert!(
        !checks.iter().any(|check| check.level >= Level::Warn),
        "{checks:?}"
    );
}

#[test]
fn a_chain_out_of_order() {
    let checks = checks("wrong-order.pem", clock());
    assert!(has(
        &checks,
        Level::Warn,
        &Finding::IssuerBefore {
            object: 2,
            issuer: 1
        }
    ));
}

#[test]
fn the_key_in_the_file_belongs_to_the_leaf() {
    let checks = checks("chain-and-key.pem", clock());
    let belongs = Finding::KeyBelongs {
        object: 3,
        holder: 1,
        signed: Signed::Certificate,
    };
    assert!(has(&checks, Level::Ok, &belongs));
    assert!(has(
        &checks,
        Level::Info,
        &Finding::KeyNotEncrypted { object: 3 }
    ));
}

#[test]
fn a_key_that_belongs_to_nothing() {
    let checks = checks("key-mismatch.pem", clock());
    assert!(has(
        &checks,
        Level::Error,
        &Finding::KeyMatchesNothing { object: 2 }
    ));
    assert!(checks.iter().any(|check| check.level == Level::Info
        && matches!(&check.finding, Finding::IssuerMissing { object: 1, name, broken: false }
            if name.contains("CN=Test Root CA"))));
}

#[test]
fn validity_against_the_clock() {
    // 2026-12-10: 22 days before the leaf's end.
    let soon = checks("leaf.crt", at(1_796_860_800));
    let expires = Finding::ExpiresSoon {
        object: 1,
        until: LEAF_END,
        days: 22,
    };
    assert!(has(&soon, Level::Warn, &expires));
    let never = checks(
        "leaf.crt",
        Clock {
            now: 1_796_860_800,
            warn_days: 0,
        },
    );
    let valid = Finding::ValidUntil {
        object: 1,
        until: LEAF_END,
        days: 22,
    };
    assert!(has(&never, Level::Ok, &valid));
    let expired = checks("leaf.crt", at(LEAF_END + 31 * 86_400));
    let gone = Finding::Expired {
        object: 1,
        until: LEAF_END,
        days: 31,
    };
    assert!(has(&expired, Level::Error, &gone));
    let early = checks("leaf.crt", at(LEAF_START - 86_400 * 3 + 1));
    let not_yet = Finding::NotYetValid {
        object: 1,
        from: LEAF_START,
        days: 3,
    };
    assert!(has(&early, Level::Error, &not_yet));
}

#[test]
fn weak_keys_and_signatures() {
    let weak = checks("weak.crt", clock());
    let small = Finding::WeakKey {
        object: 1,
        weakness: Weakness::SmallRsa(1024),
    };
    assert!(has(&weak, Level::Error, &small));
    assert!(weak.iter().any(|check| check.level == Level::Error
        && matches!(check.finding, Finding::Expired { object: 1, .. })));
    let dsa = checks("dsa.key", clock());
    let deprecated = Finding::WeakKey {
        object: 1,
        weakness: Weakness::Dsa,
    };
    assert!(has(&dsa, Level::Warn, &deprecated));
}

#[test]
fn an_encrypted_key_beside_a_certificate_cannot_be_compared() {
    let mut bytes = fixture("leaf.crt").to_vec();
    bytes.extend_from_slice(fixture("leaf-rsa-enc.key"));
    let (objects, _) = parse(&bytes, 10);
    let checks = run(&objects, clock());
    let not_comparable = Finding::KeyNotComparable {
        object: 2,
        why: NoPublic::Encrypted,
    };
    assert!(has(&checks, Level::Warn, &not_comparable));
    let encrypted = Finding::KeyEncrypted {
        object: 2,
        cipher: "PBES2: PBKDF2, AES-256-CBC".to_owned(),
    };
    assert!(has(&checks, Level::Info, &encrypted));
}

#[test]
fn damaged_and_unsupported_objects_are_lines_of_their_own() {
    let keys = checks("authorized_keys", clock());
    let damaged = Finding::Damaged {
        object: 4,
        why: Damage::NotAKeyLine,
    };
    assert!(has(&keys, Level::Error, &damaged));
    let p12 = checks("leaf.p12", clock());
    let unsupported = Finding::Unsupported {
        object: 1,
        what: Unsupported::Pkcs12,
    };
    assert!(has(&p12, Level::Info, &unsupported));
}

#[test]
fn a_single_public_key_has_nothing_to_check() {
    let checks = checks("id_ed25519.pub", clock());
    assert_eq!(
        checks,
        [Check {
            level: Level::Ok,
            finding: Finding::NothingToCheck
        }]
    );
}

/// A certificate made up for the chain rules alone.
fn made_up(subject: &[u8], issuer: &[u8], ski: u8, aki: Option<u8>, ca: bool) -> Object {
    let cert = Certificate {
        subject: String::from_utf8_lossy(subject).into_owned(),
        issuer: String::from_utf8_lossy(issuer).into_owned(),
        subject_raw: subject.to_vec(),
        issuer_raw: issuer.to_vec(),
        common_name: None,
        serial: "01".to_owned(),
        version: 3,
        not_before: 0,
        not_after: MID_2026 * 2,
        key: None,
        signature: Signature {
            name: "ecdsa-with-SHA256".to_owned(),
            strength: Strength::Good,
        },
        sha1: String::new(),
        sha256: String::new(),
        extensions: Extensions {
            subject_key_id: Some(vec![ski]),
            authority_key_id: aki.map(|id| vec![id]),
            basic_constraints: Some((ca, None)),
            ..Extensions::default()
        },
    };
    Object {
        origin: Origin::Der,
        body: Body::Certificate(Box::new(cert)),
    }
}

#[test]
fn chain_rules_on_made_up_certificates() {
    // #1 leaf by B (not a CA), #2 B by C (missing), #3 D names B as issuer with another key id.
    let objects = [
        made_up(b"A", b"B", 1, Some(2), false),
        made_up(b"B", b"C", 2, Some(3), false),
        made_up(b"D", b"B", 4, Some(9), false),
    ];
    let checks = run(&objects, clock());
    assert!(has(
        &checks,
        Level::Ok,
        &Finding::IssuedBy {
            object: 1,
            issuer: 2
        }
    ));
    assert!(has(
        &checks,
        Level::Warn,
        &Finding::IssuerNotCa {
            object: 1,
            issuer: 2
        }
    ));
    let missing = Finding::IssuerMissing {
        object: 2,
        name: "C".to_owned(),
        broken: true,
    };
    assert!(has(&checks, Level::Warn, &missing));
    assert!(has(
        &checks,
        Level::Warn,
        &Finding::KeyIdMismatch {
            object: 3,
            issuer: 2
        }
    ));
}
