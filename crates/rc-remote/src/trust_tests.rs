use rc_db::Db;

use super::*;
use crate::known_hosts_import::parse;

const ED25519: &str = include_str!("../tests/fixtures/known_hosts/ed25519.pub");
const OTHER: &str = include_str!("../tests/fixtures/known_hosts/other.pub");
const HASHED: &str = include_str!("../tests/fixtures/known_hosts/hashed_known_hosts");

fn key(pub_line: &str) -> OfferedKey {
    let mut fields = pub_line.split_whitespace();
    OfferedKey::from_openssh(fields.next().expect("type"), fields.next().expect("key"))
        .expect("a valid key")
}

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

#[tokio::test]
async fn a_trusted_first_sighting_is_trusted_next_time() {
    let db = db().await;
    assert!(matches!(
        verify(&db, "box.example", 2222, key(ED25519))
            .await
            .expect("verify"),
        Verdict::Unknown(_)
    ));
    let stored = trust(&db, "Box.Example", 2222, key(ED25519))
        .await
        .expect("trust");
    assert_eq!(stored.host, "box.example");
    assert_eq!(stored.source, KnownHostSource::User);
    let Verdict::Trusted(seen) = verify(&db, "box.example", 2222, key(ED25519))
        .await
        .expect("verify")
    else {
        panic!("trusted after the person confirmed it");
    };
    assert!(seen.last_seen_at.is_none(), "the row as it was before");
    let row = db.known_host(stored.id).await.expect("get").expect("row");
    assert!(
        row.last_seen_at.is_some(),
        "a verified key is marked as seen"
    );
}

#[tokio::test]
async fn trusting_a_changed_key_is_refused_and_keeps_the_stored_one() {
    let db = db().await;
    let old = trust(&db, "box.example", 22, key(OTHER))
        .await
        .expect("trust");
    let refused = trust(&db, "box.example", 22, key(ED25519)).await;
    assert!(
        matches!(&refused, Err(TrustError::Changed { stored_fingerprint, .. })
            if *stored_fingerprint == old.fingerprint_sha256),
        "{refused:?}"
    );
    assert_eq!(db.known_hosts().await.expect("list"), vec![old]);
}

#[tokio::test]
async fn only_the_replace_action_swaps_a_changed_key() {
    let db = db().await;
    let old = trust(&db, "box.example", 22, key(OTHER))
        .await
        .expect("trust");
    let Verdict::Changed { stored, offered } = verify(&db, "box.example", 22, key(ED25519))
        .await
        .expect("verify")
    else {
        panic!("a changed key");
    };
    let replaced = replace(&db, &stored, "box.example", 22, &offered)
        .await
        .expect("replace");
    assert_eq!(replaced.fingerprint_sha256, key(ED25519).fingerprint);
    // The decision was made on `old`; it is gone now, so it replaces nothing a second time.
    assert!(matches!(
        replace(&db, &old, "box.example", 22, &key(OTHER)).await,
        Err(TrustError::Stale)
    ));
    assert!(matches!(
        verify(&db, "box.example", 22, key(ED25519))
            .await
            .expect("verify"),
        Verdict::Trusted(_)
    ));
}

#[tokio::test]
async fn an_imported_hashed_key_blocks_a_changed_one() {
    let db = db().await;
    let summary = import(&db, &parse(HASHED)).await.expect("import");
    assert_eq!((summary.keys.added, summary.revoked), (2, 0));
    assert!(matches!(
        verify(&db, "box.example", 22, key(ED25519))
            .await
            .expect("verify"),
        Verdict::Trusted(_)
    ));
    assert!(matches!(
        trust(&db, "box.example", 22, key(OTHER)).await,
        Err(TrustError::Changed { .. })
    ));
    let again = import(&db, &parse(HASHED)).await.expect("import");
    assert_eq!((again.keys.added, again.keys.unchanged), (0, 2));
}

/// The `@revoked` line of a file for a key the line before it trusts.
fn revoking(pub_line: &str) -> String {
    let mut fields = pub_line.split_whitespace();
    let (kind, blob) = (fields.next().expect("type"), fields.next().expect("key"));
    format!("box.example {kind} {blob}\n@revoked box.example {kind} {blob}\n")
}

/// RC-080-67: an imported `@revoked` line is kept, and its key is refused before any other
/// verdict - for every host, trusted or not, and by trust and replace alike.
#[tokio::test]
async fn a_revoked_key_is_refused_before_any_trust() {
    let db = db().await;
    // Trusted by a person first: the revocation still wins.
    trust(&db, "nas.example", 22, key(ED25519))
        .await
        .expect("trusted");
    let summary = import(&db, &parse(&revoking(ED25519)))
        .await
        .expect("import");
    assert_eq!((summary.keys.added, summary.revoked), (0, 1));
    for host in ["box.example", "nas.example", "other.example"] {
        let verdict = verify(&db, host, 22, key(ED25519)).await.expect("verify");
        assert_eq!(verdict.code(), Some(crate::HOST_KEY_REVOKED), "{host}");
        let Verdict::Revoked { revoked, .. } = verdict else {
            panic!("revoked for {host}");
        };
        assert_eq!(revoked.hosts, "box.example");
    }
    assert!(matches!(
        trust(&db, "fresh.example", 22, key(ED25519)).await,
        Err(TrustError::Revoked { .. })
    ));
    let stored = trust(&db, "box.example", 22, key(OTHER))
        .await
        .expect("another key is trusted");
    assert!(matches!(
        replace(&db, &stored, "box.example", 22, &key(ED25519)).await,
        Err(TrustError::Revoked { .. })
    ));
    // Once a person removed the revocation, the key is judged as any other again.
    let row = db.revoked_host_keys().await.expect("list").remove(0);
    db.remove_revoked_host_key(row.id).await.expect("removed");
    assert!(matches!(
        verify(&db, "nas.example", 22, key(ED25519))
            .await
            .expect("verify"),
        Verdict::Trusted(_)
    ));
}
