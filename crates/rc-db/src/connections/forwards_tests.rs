//! A connection's port forwards (RC-110-03): checked, bounded, gone with the connection.

use crate::{
    Db,
    connections::{ConnectionDraft, ConnectionId, ConnectionKind, ForwardDraft, MAX_FORWARDS},
};

fn forward(port: i64) -> ForwardDraft {
    ForwardDraft {
        bind_address: String::new(),
        bind_port: port,
        target_host: " db ".to_owned(),
        target_port: 5432,
        auto_start: true,
        name: Some("prod".to_owned()),
    }
}

async fn ssh(db: &Db) -> ConnectionId {
    db.create_connection(None, &ConnectionDraft::new("web", "web.example.org"))
        .await
        .expect("web")
        .id
}

#[tokio::test]
async fn a_forward_reads_back_with_loopback_and_its_label() {
    let db = Db::open_in_memory().await.expect("open");
    let web = ssh(&db).await;
    let made = db.create_forward(web, &forward(15432)).await.expect("made");
    assert_eq!(made.bind_address, "127.0.0.1");
    assert_eq!(made.target_host, "db");
    assert!(made.auto_start);
    assert_eq!(made.label(), "127.0.0.1:15432 \u{2192} db:5432");
    assert_eq!(
        db.forwards(web).await.expect("list"),
        std::slice::from_ref(&made)
    );
    let six = ForwardDraft {
        bind_address: "::1".to_owned(),
        ..forward(15433)
    };
    let changed = db.update_forward(made.id, &six).await.expect("changed");
    assert_eq!(changed.label(), "[::1]:15433 \u{2192} db:5432");
    db.delete_forward(made.id).await.expect("deleted");
    assert_eq!(
        db.forward(made.id).await.expect_err("gone").code(),
        "forward_not_found"
    );
}

#[tokio::test]
async fn a_forward_with_a_bad_field_is_refused() {
    let db = Db::open_in_memory().await.expect("open");
    let web = ssh(&db).await;
    for bad in [
        forward(0),
        forward(70_000),
        ForwardDraft {
            target_port: 0,
            ..forward(1)
        },
        ForwardDraft {
            target_host: "two words".to_owned(),
            ..forward(1)
        },
        ForwardDraft {
            bind_address: "localhost".to_owned(),
            ..forward(1)
        },
    ] {
        let error = db.create_forward(web, &bad).await.expect_err("refused");
        assert_eq!(error.code(), "forward_invalid", "{bad:?}");
    }
    // A LAN address is stored; whether it may be bound is decided when it starts.
    let lan = ForwardDraft {
        bind_address: "0.0.0.0".to_owned(),
        ..forward(8080)
    };
    db.create_forward(web, &lan).await.expect("stored");
}

#[tokio::test]
async fn only_ssh_connections_forward_and_at_most_32_each() {
    let db = Db::open_in_memory().await.expect("open");
    let mut ftp = ConnectionDraft::new("files", "files.example.org");
    ftp.kind = ConnectionKind::Ftp;
    ftp.username = Some("me".to_owned());
    let ftp = db.create_connection(None, &ftp).await.expect("ftp");
    let error = db
        .create_forward(ftp.id, &forward(1))
        .await
        .expect_err("FTP");
    assert_eq!(error.code(), "forward_invalid");
    let web = ssh(&db).await;
    for port in 1..=i64::try_from(MAX_FORWARDS).expect("small") {
        db.create_forward(web, &forward(port))
            .await
            .expect("within");
    }
    let error = db
        .create_forward(web, &forward(9999))
        .await
        .expect_err("one too many");
    assert_eq!(error.code(), "forwards_too_many");
}

#[tokio::test]
async fn the_forwards_go_with_their_connection() {
    let db = Db::open_in_memory().await.expect("open");
    let web = ssh(&db).await;
    db.create_forward(web, &forward(1)).await.expect("made");
    db.delete_connection(web).await.expect("deleted");
    assert!(db.forwards(web).await.expect("list").is_empty());
}
