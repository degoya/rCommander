use super::*;

fn key(fingerprint: &str) -> NewRevokedHostKey {
    NewRevokedHostKey {
        algorithm: "ssh-ed25519".to_owned(),
        public_key: format!("blob-{fingerprint}"),
        fingerprint_sha256: format!("SHA256:{fingerprint}"),
        hosts: "box.example".to_owned(),
        source: "/home/me/.ssh/known_hosts".to_owned(),
    }
}

#[tokio::test]
async fn revoked_keys_are_stored_once_found_by_fingerprint_and_removed() {
    let db = Db::open_in_memory().await.expect("open");
    assert_eq!(db.add_revoked_host_keys(&[]).await.expect("none"), 0);
    let added = db
        .add_revoked_host_keys(&[key("AAA"), key("BBB"), key("AAA")])
        .await
        .expect("add");
    assert_eq!(added, 2, "the same key twice is one row");
    assert_eq!(
        db.add_revoked_host_keys(&[key("BBB")])
            .await
            .expect("again"),
        0
    );

    let all = db.revoked_host_keys().await.expect("list");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].hosts, "box.example");
    assert!(!all[0].added_at.is_empty());
    let found = db
        .revoked_host_key("SHA256:AAA")
        .await
        .expect("read")
        .expect("revoked");
    assert_eq!(found.public_key, "blob-AAA");
    assert_eq!(db.revoked_host_key("SHA256:CCC").await.expect("read"), None);

    let removed = db
        .remove_revoked_host_key(found.id)
        .await
        .expect("remove")
        .expect("a row");
    assert_eq!(removed, found);
    assert_eq!(
        db.remove_revoked_host_key(found.id).await.expect("gone"),
        None
    );
    assert_eq!(db.revoked_host_key("SHA256:AAA").await.expect("read"), None);
}

#[tokio::test]
async fn a_row_without_a_sha256_fingerprint_is_refused() {
    let db = Db::open_in_memory().await.expect("open");
    let mut bad = key("AAA");
    bad.fingerprint_sha256 = "MD5:aa".to_owned();
    assert!(db.add_revoked_host_keys(&[bad, key("BBB")]).await.is_err());
    assert!(
        db.revoked_host_keys().await.expect("list").is_empty(),
        "one transaction"
    );
}
