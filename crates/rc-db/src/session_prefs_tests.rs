use super::*;
use crate::connections::ConnectionDraft;

async fn db_with_connection() -> (Db, ConnectionId) {
    let db = Db::open_in_memory().await.expect("db");
    let connection = db
        .create_connection(None, &ConnectionDraft::new("web", "example.org"))
        .await
        .expect("connection");
    (db, connection.id)
}

fn prefs() -> StoredSessionPrefs {
    StoredSessionPrefs {
        auth_order: "password,agent".to_owned(),
        agent_fingerprint: Some("SHA256:abc".to_owned()),
        keepalive_secs: 15,
        idle_close_secs: 0,
    }
}

#[tokio::test]
async fn a_connection_without_settings_has_none() {
    let (db, id) = db_with_connection().await;
    assert_eq!(db.session_prefs(id).await.expect("read"), None);
}

#[tokio::test]
async fn settings_are_stored_and_replaced() {
    let (db, id) = db_with_connection().await;
    db.set_session_prefs(id, &prefs()).await.expect("stored");
    assert_eq!(db.session_prefs(id).await.expect("read"), Some(prefs()));
    let changed = StoredSessionPrefs {
        agent_fingerprint: None,
        idle_close_secs: 600,
        ..prefs()
    };
    db.set_session_prefs(id, &changed).await.expect("replaced");
    assert_eq!(db.session_prefs(id).await.expect("read"), Some(changed));
}

#[tokio::test]
async fn settings_of_a_missing_connection_are_refused() {
    let (db, _) = db_with_connection().await;
    let error = db
        .set_session_prefs(ConnectionId(999), &prefs())
        .await
        .expect_err("no such connection");
    assert_eq!(error.code(), "connection_not_found");
}

#[tokio::test]
async fn the_settings_go_with_their_connection() {
    let (db, id) = db_with_connection().await;
    db.set_session_prefs(id, &prefs()).await.expect("stored");
    db.delete_connection(id).await.expect("deleted");
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM connection_session_prefs")
        .fetch_one(db.readers())
        .await
        .expect("count");
    assert_eq!(left, 0);
}
