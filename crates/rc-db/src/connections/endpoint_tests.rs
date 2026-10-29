use crate::{
    Db,
    connections::{
        Connection, ConnectionDraft, ConnectionError, ConnectionId, ConnectionPatch,
        ConnectionSave, EndpointRule, SaveError, SaveTarget,
    },
    login_scripts::LoginScriptFields,
};

/// The edit of a save that rewrites no setting.
type NoEdit = fn(Option<bool>, &Connection, &[ConnectionId]) -> bool;

const KEEP: EndpointRule = EndpointRule::KeepWhileSecretsHeld;

fn host(to: &str) -> ConnectionPatch {
    ConnectionPatch {
        host: Some(to.to_owned()),
        ..ConnectionPatch::default()
    }
}

async fn connection(db: &Db, name: &str) -> ConnectionId {
    db.create_connection(None, &ConnectionDraft::new(name, "example.org"))
        .await
        .expect("connection")
        .id
}

/// A stored password, as the vault's ciphertext would sit in the row.
async fn store_password(db: &Db, id: ConnectionId) {
    let mut writer = db.writer().await;
    sqlx::query("UPDATE connections SET password_enc = x'00' WHERE id = ?")
        .bind(id.0)
        .execute(&mut *writer)
        .await
        .expect("password");
}

/// `id` runs a script of `steps`.
async fn run_script(db: &Db, id: ConnectionId, name: &str, steps: &str) {
    let script = db
        .create_login_script(&LoginScriptFields {
            name,
            schema_version: 1,
            steps,
            step_timeout_ms: 15_000,
            total_timeout_ms: 120_000,
        })
        .await
        .expect("script")
        .id;
    let mut writer = db.writer().await;
    sqlx::query("UPDATE connections SET login_script_id = ? WHERE id = ?")
        .bind(script.0)
        .bind(id.0)
        .execute(&mut *writer)
        .await
        .expect("assigned");
}

fn refused(result: Result<Connection, ConnectionError>, id: ConnectionId) {
    assert!(
        matches!(result, Err(ConnectionError::EndpointChangeNeedsGui { id: held }) if held == id),
        "{result:?}"
    );
}

#[tokio::test]
async fn a_connection_holding_a_secret_keeps_its_endpoint_under_the_rule() {
    let db = Db::open_in_memory().await.expect("open");
    let id = connection(&db, "web").await;
    // Nothing held yet: the endpoint moves.
    db.update_connection_with(id, &host("a.example.org"), KEEP)
        .await
        .expect("no secret, free to move");

    store_password(&db, id).await;
    for patch in [
        host("evil.example.org"),
        ConnectionPatch {
            port: Some(2222),
            ..ConnectionPatch::default()
        },
        ConnectionPatch {
            username: Some("root".to_owned()),
            ..ConnectionPatch::default()
        },
    ] {
        refused(db.update_connection_with(id, &patch, KEEP).await, id);
    }
    assert_eq!(
        db.connection(id).await.expect("read").host,
        "a.example.org",
        "nothing written"
    );
    let renamed = ConnectionPatch {
        name: Some("web2".to_owned()),
        ..ConnectionPatch::default()
    };
    db.update_connection_with(id, &renamed, KEEP)
        .await
        .expect("a rename is no endpoint change");
    db.update_connection_with(id, &host("b.example.org"), KEEP)
        .await
        .expect_err("still held");
    db.update_connection(id, &host("b.example.org"))
        .await
        .expect("the window's change is free");
}

#[tokio::test]
async fn a_login_script_that_sends_a_secret_counts_as_holding_one() {
    let db = Db::open_in_memory().await.expect("open");
    let typed = connection(&db, "typed").await;
    run_script(
        &db,
        typed,
        "typed",
        r#"[{"step":"send_text","text":"sudo -i","newline":true}]"#,
    )
    .await;
    db.update_connection_with(typed, &host("a.example.org"), KEEP)
        .await
        .expect("a script without a secret moves");

    let secret = connection(&db, "secret").await;
    run_script(
        &db,
        secret,
        "secret",
        r#"[{"step":"wait_for","pattern":"word:"},{"step":"send_secret","source":1}]"#,
    )
    .await;
    refused(
        db.update_connection_with(secret, &host("a.example.org"), KEEP)
            .await,
        secret,
    );
}

/// The save of the manager's form judges in its own transaction too, and writes nothing then.
#[tokio::test]
async fn a_save_refuses_the_endpoint_change_before_it_writes() {
    let db = Db::open_in_memory().await.expect("open");
    let id = connection(&db, "web").await;
    store_password(&db, id).await;
    let patch = host("evil.example.org");
    let save = ConnectionSave {
        target: SaveTarget::Update { id, patch: &patch },
        secrets: Vec::new(),
        login_script: None,
        credential: None,
    };
    let result = db
        .save_connection_with(
            &save,
            KEEP,
            |_, _| Ok::<Vec<u8>, ()>(Vec::new()),
            None::<(&str, NoEdit)>,
        )
        .await;
    assert!(
        matches!(
            result,
            Err(SaveError::Connection(
                ConnectionError::EndpointChangeNeedsGui { .. }
            ))
        ),
        "{result:?}"
    );
    assert_eq!(db.connection(id).await.expect("read").host, "example.org");
    db.save_connection(
        &save,
        |_, _| Ok::<Vec<u8>, ()>(Vec::new()),
        None::<(&str, NoEdit)>,
    )
    .await
    .expect("free without the rule");
}
