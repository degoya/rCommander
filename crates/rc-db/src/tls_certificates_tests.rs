use super::*;

fn certificate(host: &str, port: u16, byte: u8) -> NewTrustedCertificate {
    NewTrustedCertificate {
        host: host.to_owned(),
        port,
        fingerprint_sha256: vec![format!("{byte:02X}"); 32].join(":"),
        subject: "CN=nas.example".to_owned(),
        issuer: "CN=nas.example".to_owned(),
        not_after: 1_900_000_000,
    }
}

async fn db() -> Db {
    Db::open_in_memory().await.expect("open")
}

#[tokio::test]
async fn a_trusted_certificate_is_stored_found_and_listed() {
    let db = db().await;
    let stored = db
        .add_trusted_certificate(&certificate("nas.example", 990, 0xAB))
        .await
        .expect("add")
        .expect("new row");
    assert_eq!(stored.port, 990);
    assert_eq!(stored.not_after, 1_900_000_000);
    assert_eq!(stored.last_seen_at, None);
    assert_eq!(
        db.trusted_certificate_for("nas.example", 990)
            .await
            .expect("get"),
        Some(stored.clone())
    );
    assert_eq!(
        db.trusted_certificate_for("nas.example", 21)
            .await
            .expect("get"),
        None,
        "another port is another server"
    );
    assert_eq!(db.trusted_certificates().await.expect("list"), vec![stored]);
}

#[tokio::test]
async fn adding_never_replaces_a_stored_certificate() {
    let db = db().await;
    let first = db
        .add_trusted_certificate(&certificate("nas.example", 21, 0x01))
        .await
        .expect("add")
        .expect("new row");
    let again = db
        .add_trusted_certificate(&certificate("nas.example", 21, 0x02))
        .await
        .expect("add");
    assert_eq!(
        again, None,
        "a changed certificate is not stored over the old one"
    );
    assert_eq!(db.trusted_certificates().await.expect("list"), vec![first]);
}

#[tokio::test]
async fn removing_frees_the_place_and_touching_notes_the_time() {
    let db = db().await;
    let first = db
        .add_trusted_certificate(&certificate("nas.example", 21, 0x01))
        .await
        .expect("add")
        .expect("new row");
    db.touch_trusted_certificate(first.id).await.expect("touch");
    let touched = db
        .trusted_certificate_for("nas.example", 21)
        .await
        .expect("get")
        .expect("stored");
    assert!(touched.last_seen_at.is_some());

    let removed = db
        .remove_trusted_certificate(first.id)
        .await
        .expect("remove")
        .expect("a row");
    assert_eq!(removed.id, first.id);
    assert_eq!(
        db.remove_trusted_certificate(first.id)
            .await
            .expect("remove"),
        None
    );
    assert!(
        db.add_trusted_certificate(&certificate("nas.example", 21, 0x02))
            .await
            .expect("add")
            .is_some()
    );
}

#[tokio::test]
async fn a_malformed_fingerprint_is_refused_by_the_table() {
    let db = db().await;
    let mut bad = certificate("nas.example", 21, 0x01);
    bad.fingerprint_sha256 = "SHA256:x".to_owned();
    assert!(db.add_trusted_certificate(&bad).await.is_err());
}
