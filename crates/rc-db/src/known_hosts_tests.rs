use super::*;

fn key(host: &str, port: u16, fingerprint: &str) -> NewKnownHost {
    NewKnownHost {
        host: host.to_owned(),
        port,
        algorithm: "ssh-ed25519".to_owned(),
        public_key: format!("blob-{fingerprint}"),
        fingerprint_sha256: format!("SHA256:{fingerprint}"),
        source: KnownHostSource::User,
    }
}

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

#[tokio::test]
async fn a_first_sighting_is_stored_and_listed() {
    let db = db().await;
    let stored = db
        .add_known_host(&key("box.example", 2222, "AAA"))
        .await
        .expect("add")
        .expect("new row");
    assert_eq!(stored.port, 2222);
    assert_eq!(stored.source, KnownHostSource::User);
    assert_eq!(stored.last_seen_at, None);
    assert!(!stored.is_hashed());
    assert_eq!(db.known_hosts().await.expect("list"), vec![stored.clone()]);
    assert_eq!(
        db.known_host(stored.id).await.expect("get"),
        Some(stored.clone())
    );
}

#[tokio::test]
async fn adding_never_replaces_a_stored_key() {
    let db = db().await;
    let first = db
        .add_known_host(&key("box.example", 22, "OLD"))
        .await
        .expect("add")
        .expect("new row");
    let again = db
        .add_known_host(&key("box.example", 22, "NEW"))
        .await
        .expect("add");
    assert_eq!(again, None, "a changed key is not a first sighting");
    let rows = db.known_hosts().await.expect("list");
    assert_eq!(rows, vec![first]);
}

#[tokio::test]
async fn the_same_host_on_another_port_is_another_row() {
    let db = db().await;
    db.add_known_host(&key("box.example", 22, "A"))
        .await
        .expect("add")
        .expect("new");
    db.add_known_host(&key("box.example", 2222, "B"))
        .await
        .expect("add")
        .expect("new");
    let on_22 = db
        .known_host_candidates("box.example", 22)
        .await
        .expect("candidates");
    assert_eq!(on_22.len(), 1);
    assert_eq!(on_22[0].fingerprint_sha256, "SHA256:A");
}

#[tokio::test]
async fn hashed_rows_are_candidates_for_every_host() {
    let db = db().await;
    db.add_known_host(&key("|1|c2FsdA==|aGFzaA==", 0, "H"))
        .await
        .expect("add")
        .expect("new");
    db.add_known_host(&key("other.example", 22, "O"))
        .await
        .expect("add")
        .expect("new");
    let found = db
        .known_host_candidates("box.example", 22)
        .await
        .expect("candidates");
    assert_eq!(found.len(), 1);
    assert!(found[0].is_hashed());
}

#[tokio::test]
async fn a_replacement_must_name_the_key_it_replaces() {
    let db = db().await;
    let old = db
        .add_known_host(&key("box.example", 22, "OLD"))
        .await
        .expect("add")
        .expect("new");
    let stale = db
        .replace_known_host(
            old.id,
            "SHA256:SOMETHING-ELSE",
            &key("box.example", 22, "NEW"),
        )
        .await
        .expect("replace");
    assert_eq!(stale, None);
    assert_eq!(db.known_hosts().await.expect("list"), vec![old.clone()]);

    let replaced = db
        .replace_known_host(old.id, "SHA256:OLD", &key("box.example", 22, "NEW"))
        .await
        .expect("replace")
        .expect("replaced");
    assert_eq!(replaced.fingerprint_sha256, "SHA256:NEW");
    assert_eq!(db.known_hosts().await.expect("list"), vec![replaced]);
}

#[tokio::test]
async fn touching_and_removing() {
    let db = db().await;
    let stored = db
        .add_known_host(&key("box.example", 22, "A"))
        .await
        .expect("add")
        .expect("new");
    db.touch_known_host(stored.id).await.expect("touch");
    let seen = db.known_host(stored.id).await.expect("get").expect("row");
    assert!(seen.last_seen_at.is_some());
    let removed = db.remove_known_host(stored.id).await.expect("remove");
    assert_eq!(removed.map(|row| row.id), Some(stored.id));
    assert_eq!(db.remove_known_host(stored.id).await.expect("remove"), None);
    assert!(db.known_hosts().await.expect("list").is_empty());
}

#[tokio::test]
async fn an_import_counts_and_leaves_conflicts_alone() {
    let db = db().await;
    db.add_known_host(&key("kept.example", 22, "MINE"))
        .await
        .expect("add")
        .expect("new");
    let mut imported = vec![
        key("new.example", 22, "N"),
        key("kept.example", 22, "THEIRS"),
        key("new.example", 22, "N"),
    ];
    for entry in &mut imported {
        entry.source = KnownHostSource::Import;
    }
    let summary = db.import_known_hosts(&imported).await.expect("import");
    assert_eq!(summary.added, 1);
    assert_eq!(summary.unchanged, 1);
    assert_eq!(summary.conflicts, vec![imported[1].clone()]);
    let kept = db
        .known_host_candidates("kept.example", 22)
        .await
        .expect("candidates");
    assert_eq!(kept[0].fingerprint_sha256, "SHA256:MINE");
    let new = db
        .known_host_candidates("new.example", 22)
        .await
        .expect("candidates");
    assert_eq!(new[0].source, KnownHostSource::Import);
}

#[tokio::test]
async fn the_table_refuses_a_fingerprint_in_another_form() {
    let db = db().await;
    let mut wrong = key("box.example", 22, "A");
    wrong.fingerprint_sha256 = "MD5:aa:bb".to_owned();
    assert!(db.add_known_host(&wrong).await.is_err());
}
