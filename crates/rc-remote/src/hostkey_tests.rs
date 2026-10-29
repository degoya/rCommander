use rc_db::{KnownHost, KnownHostSource};
use russh::keys::{Algorithm, EcdsaCurve, HashAlg, PublicKey, PublicKeyOrCertificate};

use super::*;

const ED25519: &str = include_str!("../tests/fixtures/known_hosts/ed25519.pub");
const ECDSA: &str = include_str!("../tests/fixtures/known_hosts/ecdsa.pub");
const RSA: &str = include_str!("../tests/fixtures/known_hosts/rsa.pub");
const OTHER: &str = include_str!("../tests/fixtures/known_hosts/other.pub");
/// `ssh-keygen -lf` of the three fixture keys, one line each.
const FINGERPRINTS: &str = include_str!("../tests/fixtures/known_hosts/fingerprints.txt");
/// `ssh-keygen -lv` of the fixture keys: the randomart below the first line.
const ED25519_ART: &str = include_str!("../tests/fixtures/known_hosts/ed25519.randomart");
const ECDSA_ART: &str = include_str!("../tests/fixtures/known_hosts/ecdsa.randomart");
const RSA_ART: &str = include_str!("../tests/fixtures/known_hosts/rsa.randomart");

/// The key of a `.pub` line (`<type> <base64> <comment>`).
fn offered(pub_line: &str) -> OfferedKey {
    let mut fields = pub_line.split_whitespace();
    let algorithm = fields.next().expect("type");
    let base64 = fields.next().expect("key");
    OfferedKey::from_openssh(algorithm, base64).expect("a valid key")
}

fn stored(host: &str, port: u16, key: &OfferedKey, id: i64) -> KnownHost {
    KnownHost {
        id,
        host: host.to_owned(),
        port,
        algorithm: key.algorithm.clone(),
        public_key: key.public_key.clone(),
        fingerprint_sha256: key.fingerprint.clone(),
        added_at: "2026-10-17T00:00:00.000Z".to_owned(),
        last_seen_at: None,
        source: KnownHostSource::User,
    }
}

#[test]
fn fingerprints_are_what_ssh_keygen_prints() {
    let expected: Vec<&str> = FINGERPRINTS
        .lines()
        .map(|line| line.split_whitespace().nth(1).expect("fingerprint column"))
        .collect();
    let computed: Vec<String> = [ED25519, ECDSA, RSA]
        .iter()
        .map(|line| offered(line).fingerprint)
        .collect();
    assert_eq!(computed, expected);
    assert_eq!(offered(ED25519).algorithm, "ssh-ed25519");
    assert_eq!(offered(ECDSA).algorithm, "ecdsa-sha2-nistp256");
    assert_eq!(offered(RSA).algorithm, "ssh-rsa");
}

#[test]
fn the_blob_is_kept_as_known_hosts_spells_it() {
    for line in [ED25519, ECDSA, RSA] {
        let base64 = line.split_whitespace().nth(1).expect("key");
        assert_eq!(offered(line).public_key, base64);
    }
}

#[test]
fn randomart_is_what_ssh_keygen_draws() {
    for (line, art) in [(ED25519, ED25519_ART), (ECDSA, ECDSA_ART), (RSA, RSA_ART)] {
        assert_eq!(
            offered(line).randomart().expect("parses").trim_end(),
            art.trim_end()
        );
    }
}

#[test]
fn a_type_that_is_not_the_blobs_is_refused() {
    let base64 = ED25519.split_whitespace().nth(1).expect("key");
    assert_eq!(
        OfferedKey::from_openssh("ssh-rsa", base64),
        Err(KeyError::Invalid)
    );
    assert_eq!(
        OfferedKey::from_openssh("ssh-ed25519", "bm90IGEga2V5"),
        Err(KeyError::Invalid)
    );
}

#[test]
fn describe_reads_a_presented_key() {
    let key = PublicKey::from_openssh(ED25519.trim()).expect("parses");
    let presented = PublicKeyOrCertificate::PublicKey {
        key,
        hash_alg: None,
    };
    assert_eq!(describe(&presented), Ok(offered(ED25519)));
    assert_eq!(KeyError::Certificate.code(), HOST_KEY_UNSUPPORTED);
}

#[test]
fn a_known_key_is_trusted() {
    let key = offered(ED25519);
    let rows = vec![stored("box.example", 22, &key, 1)];
    let verdict = decide(&rows, "box.example", 22, key);
    assert_eq!(verdict, Verdict::Trusted(rows[0].clone()));
    assert_eq!(verdict.code(), None);
}

#[test]
fn a_known_key_on_a_non_standard_port_is_trusted_there_only() {
    let key = offered(ECDSA);
    let rows = vec![stored("box.example", 2222, &key, 1)];
    assert!(matches!(
        decide(&rows, "box.example", 2222, key.clone()),
        Verdict::Trusted(_)
    ));
    assert_eq!(
        decide(&rows, "box.example", 22, key.clone()),
        Verdict::Unknown(key.clone())
    );
    assert_eq!(
        decide(&rows, "box.example", 2223, key.clone()),
        Verdict::Unknown(key)
    );
}

#[test]
fn an_unknown_key_names_its_fingerprint() {
    let key = offered(ED25519);
    let verdict = decide(&[], "box.example", 22, key.clone());
    assert_eq!(verdict, Verdict::Unknown(key.clone()));
    assert_eq!(verdict.code(), Some(HOST_KEY_UNKNOWN));
    // The user has to be able to read the fingerprint to confirm it.
    assert!(key.fingerprint.starts_with("SHA256:"));
}

#[test]
fn a_changed_key_is_reported_with_both_fingerprints() {
    let old = offered(OTHER);
    let new = offered(ED25519);
    for port in [22, 2222] {
        let rows = vec![stored("box.example", port, &old, 7)];
        let verdict = decide(&rows, "box.example", port, new.clone());
        assert_eq!(verdict.code(), Some(HOST_KEY_CHANGED));
        let Verdict::Changed { stored, offered } = verdict else {
            panic!("a changed key is not a first sighting");
        };
        assert_eq!(stored.fingerprint_sha256, old.fingerprint);
        assert_eq!(offered.fingerprint, new.fingerprint);
    }
}

#[test]
fn keys_are_compared_per_type_and_host_names_ignore_case() {
    let ed = offered(ED25519);
    let ecdsa = offered(ECDSA);
    let rows = vec![stored("box.example", 22, &ed, 1)];
    // Another type is a first sighting of that type, as in OpenSSH.
    assert_eq!(
        decide(&rows, "box.example", 22, ecdsa.clone()),
        Verdict::Unknown(ecdsa)
    );
    assert!(matches!(
        decide(&rows, "Box.Example", 22, ed),
        Verdict::Trusted(_)
    ));
}

#[test]
fn a_second_key_of_the_same_type_still_trusts_the_one_that_matches() {
    let old = offered(OTHER);
    let new = offered(ED25519);
    let rows = vec![
        stored("box.example", 22, &old, 1),
        stored(
            "|1|MDEyMzQ1Njc4OWFiY2RlZmdoaWo=|Jo2O/bEngyhpxDCBT4DDL9yMUoE=",
            0,
            &new,
            2,
        ),
    ];
    assert_eq!(
        decide(&rows, "box.example", 22, new),
        Verdict::Trusted(rows[1].clone())
    );
}

#[test]
fn a_hashed_row_counts_for_its_host_and_port() {
    let old = offered(OTHER);
    let new = offered(ED25519);
    // The hash of `box.example` (salt `0123456789abcdefghij`, see hashed_tests.rs).
    let rows = vec![stored(
        "|1|MDEyMzQ1Njc4OWFiY2RlZmdoaWo=|Jo2O/bEngyhpxDCBT4DDL9yMUoE=",
        0,
        &old,
        3,
    )];
    assert!(matches!(
        decide(&rows, "box.example", 22, old.clone()),
        Verdict::Trusted(_)
    ));
    assert!(matches!(
        decide(&rows, "box.example", 22, new.clone()),
        Verdict::Changed { .. }
    ));
    assert_eq!(
        decide(&rows, "box.example", 2222, new.clone()),
        Verdict::Unknown(new)
    );
}

#[test]
fn host_patterns_follow_known_hosts() {
    assert_eq!(host_pattern("Box.Example", 22), "box.example");
    assert_eq!(host_pattern("box.example", 2222), "[box.example]:2222");
}

#[test]
fn all_codes_lists_every_key_error_code_once() {
    let samples = [KeyError::Certificate, KeyError::Invalid];
    for sample in &samples {
        match sample {
            KeyError::Certificate | KeyError::Invalid => {}
        }
    }
    let codes: Vec<&str> = samples.iter().map(KeyError::code).collect();
    assert_eq!(codes, KeyError::ALL_CODES);
}

/// RC-080-12 (X3): the types stored for a host, each once and only its own; the key exchange
/// asks for them first, RSA in all its signature variants.
#[test]
fn stored_types_lead_the_key_exchange() {
    let rows = [
        stored("box.example", 22, &offered(ECDSA), 1),
        stored("box.example", 22, &offered(RSA), 2),
        stored("box.example", 2222, &offered(ED25519), 3),
        stored("other.example", 22, &offered(ED25519), 4),
        stored("box.example", 22, &offered(OTHER), 5),
    ];
    // `OTHER` is a second ed25519 key: its type is named once.
    assert_eq!(
        stored_types(&rows, "Box.Example", 22),
        ["ecdsa-sha2-nistp256", "ssh-rsa", "ssh-ed25519"]
    );
    assert!(stored_types(&rows, "new.example", 22).is_empty());

    let defaults = [
        Algorithm::Ed25519,
        Algorithm::Ecdsa {
            curve: EcdsaCurve::NistP256,
        },
        Algorithm::Rsa {
            hash: Some(HashAlg::Sha512),
        },
        Algorithm::Rsa { hash: None },
    ];
    let preferred = preferred_key_algorithms(&["ssh-rsa".to_owned()], &defaults);
    assert_eq!(
        preferred,
        [
            defaults[2].clone(),
            defaults[3].clone(),
            defaults[0].clone(),
            defaults[1].clone()
        ]
    );
    assert_eq!(preferred_key_algorithms(&[], &defaults), defaults);
}
