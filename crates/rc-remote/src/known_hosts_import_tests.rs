use rc_db::{KnownHost, KnownHostSource};

use super::*;
use crate::hostkey::{Verdict, decide};

const PLAIN: &str = include_str!("../tests/fixtures/known_hosts/known_hosts");
/// `box.example` (ed25519) and `[box.example]:2222` (ecdsa), hashed by `ssh-keygen -H`.
const HASHED: &str = include_str!("../tests/fixtures/known_hosts/hashed_known_hosts");
const ED25519: &str = include_str!("../tests/fixtures/known_hosts/ed25519.pub");
const ECDSA: &str = include_str!("../tests/fixtures/known_hosts/ecdsa.pub");
const OTHER: &str = include_str!("../tests/fixtures/known_hosts/other.pub");

fn key(pub_line: &str) -> OfferedKey {
    let mut fields = pub_line.split_whitespace();
    OfferedKey::from_openssh(fields.next().expect("type"), fields.next().expect("key"))
        .expect("a valid key")
}

/// The parsed entries as stored rows, as the trust store would hand them back.
fn rows(parsed: &ParsedKnownHosts) -> Vec<KnownHost> {
    parsed
        .entries
        .iter()
        .zip(1..)
        .map(|(entry, id)| KnownHost {
            id,
            host: entry.host.clone(),
            port: entry.port,
            algorithm: entry.algorithm.clone(),
            public_key: entry.public_key.clone(),
            fingerprint_sha256: entry.fingerprint_sha256.clone(),
            added_at: "2026-10-17T00:00:00.000Z".to_owned(),
            last_seen_at: None,
            source: entry.source,
        })
        .collect()
}

#[test]
fn plain_entries_become_one_row_per_name() {
    let parsed = parse(PLAIN);
    let names: Vec<(&str, u16, &str)> = parsed
        .entries
        .iter()
        .map(|entry| (entry.host.as_str(), entry.port, entry.algorithm.as_str()))
        .collect();
    assert_eq!(
        names,
        vec![
            ("box.example", 22, "ssh-ed25519"),
            ("10.0.0.5", 22, "ssh-ed25519"),
            ("box.example", 2222, "ecdsa-sha2-nistp256"),
            ("mixed.example", 22, "ssh-rsa"),
        ]
    );
    assert!(
        parsed
            .entries
            .iter()
            .all(|entry| entry.source == KnownHostSource::Import)
    );
    assert_eq!(
        parsed.entries[0].fingerprint_sha256,
        key(ED25519).fingerprint
    );
}

#[test]
fn markers_patterns_and_broken_lines_are_reported() {
    let parsed = parse(PLAIN);
    let skipped: Vec<(usize, SkipReason, Option<&str>)> = parsed
        .skipped
        .iter()
        .map(|skip| (skip.line, skip.reason, skip.name.as_deref()))
        .collect();
    assert_eq!(
        skipped,
        vec![
            (6, SkipReason::Revoked, None),
            (7, SkipReason::CertAuthority, None),
            (8, SkipReason::Pattern, Some("*.wild.example")),
            (9, SkipReason::Pattern, Some("!nope.example")),
            (10, SkipReason::InvalidKey, None),
            (11, SkipReason::Malformed, None),
            // RC-080-12: the key line 6 revokes, on a plain line too.
            (9, SkipReason::Revoked, Some("neg.example")),
        ]
    );
}

#[test]
fn a_hashed_entry_is_kept_hashed_and_matched_on_verify() {
    let parsed = parse(HASHED);
    assert!(parsed.skipped.is_empty(), "{:?}", parsed.skipped);
    assert_eq!(parsed.entries.len(), 2);
    assert!(
        parsed
            .entries
            .iter()
            .all(|entry| entry.host.starts_with("|1|") && entry.port == 0)
    );
    let stored = rows(&parsed);

    assert!(matches!(
        decide(&stored, "box.example", 22, key(ED25519)),
        Verdict::Trusted(row) if row.id == 1
    ));
    assert!(matches!(
        decide(&stored, "box.example", 2222, key(ECDSA)),
        Verdict::Trusted(row) if row.id == 2
    ));
    assert!(matches!(
        decide(&stored, "box.example", 22, key(OTHER)),
        Verdict::Changed { .. }
    ));
    assert_eq!(
        decide(&stored, "elsewhere.example", 22, key(ED25519)),
        Verdict::Unknown(key(ED25519))
    );
}

#[test]
fn names_and_ports_read_like_openssh() {
    assert_eq!(
        host_and_port("[Box.Example]:2222"),
        Ok(("box.example".to_owned(), 2222))
    );
    assert_eq!(host_and_port("[::1]:22"), Ok(("::1".to_owned(), 22)));
    for broken in [
        "[box.example]",
        "[box.example]:x",
        "[box.example]:0",
        "|1|nope",
    ] {
        assert_eq!(
            host_and_port(broken),
            Err(SkipReason::Malformed),
            "{broken}"
        );
    }
    assert_eq!(host_and_port("?ox.example"), Err(SkipReason::Pattern));
}

#[tokio::test]
async fn read_takes_a_file_and_reports_a_missing_one() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("known_hosts");
    std::fs::write(&path, HASHED).expect("write");
    assert_eq!(read(&path).await.expect("read").entries.len(), 2);
    assert!(read(&dir.path().join("missing")).await.is_err());
}

/// RC-080-12 (X7): a key a `@revoked` line names is imported from no line, before or after it;
/// another key of the same host still is.
#[test]
fn a_revoked_key_is_imported_from_no_line() {
    let line = |prefix: &str, host: &str, pub_line: &str| {
        let mut fields = pub_line.split_whitespace();
        format!(
            "{prefix}{host} {} {}\n",
            fields.next().expect("type"),
            fields.next().expect("key")
        )
    };
    let text = [
        line("", "box.example", ED25519),
        line("@revoked ", "*", ED25519),
        line("", "late.example", ED25519),
        line("", "box.example", ECDSA),
    ]
    .concat();
    let parsed = parse(&text);
    let hosts: Vec<(&str, &str)> = parsed
        .entries
        .iter()
        .map(|entry| (entry.host.as_str(), entry.algorithm.as_str()))
        .collect();
    assert_eq!(hosts, [("box.example", "ecdsa-sha2-nistp256")]);
    let skipped: Vec<(usize, SkipReason, Option<&str>)> = parsed
        .skipped
        .iter()
        .map(|skip| (skip.line, skip.reason, skip.name.as_deref()))
        .collect();
    assert_eq!(
        skipped,
        [
            (2, SkipReason::Revoked, None),
            (1, SkipReason::Revoked, Some("box.example")),
            (3, SkipReason::Revoked, Some("late.example")),
        ]
    );
}
