//! Stored secrets are bound to their endpoint: an MCP command never moves a connection that
//! holds one to another host, port, kind or user (H3).

use super::tests::{MASTER, connection, engine_in, master};
use crate::test_support;
use crate::{
    AuthMethod, Command, ConnectionId, ConnectionPatch, Engine, Outcome, SecretKind, SecretParam,
};

async fn stored(engine: &Engine, id: ConnectionId) -> rc_db::connections::Connection {
    match engine.dispatch(Command::GetConnection { id }).await {
        Ok(Outcome::Connection { connection }) => connection,
        other => panic!("a connection, got {other:?}"),
    }
}

fn update(id: ConnectionId, patch: ConnectionPatch) -> Command {
    Command::UpdateConnection { id, patch }
}

fn to_host(host: &str) -> ConnectionPatch {
    ConnectionPatch {
        host: Some(host.to_owned()),
        ..ConnectionPatch::default()
    }
}

#[tokio::test]
async fn mcp_never_moves_a_connection_with_a_stored_secret() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let web = connection(&engine, "web", AuthMethod::Password).await;
    let bare = connection(&engine, "bare", AuthMethod::Password).await;
    engine
        .dispatch(Command::CreateVault {
            password: master(MASTER),
        })
        .await
        .expect("vault");
    engine
        .dispatch(test_support::store_secret(
            web,
            SecretKind::Password,
            SecretParam::from("hunter2-web"),
        ))
        .await
        .expect("stored");
    let before = stored(&engine, web).await;

    for patch in [
        to_host("attacker.example"),
        ConnectionPatch {
            port: Some(2222),
            ..ConnectionPatch::default()
        },
        ConnectionPatch {
            username: Some("mallory".to_owned()),
            ..ConnectionPatch::default()
        },
    ] {
        let refused = engine
            .dispatch_mcp(update(web, patch.clone()))
            .await
            .expect_err("refused");
        assert_eq!(refused.code(), "endpoint_change_needs_gui", "{patch:?}");
    }
    assert_eq!(stored(&engine, web).await, before, "the row is unchanged");

    // What leaves the endpoint as it is stays an agent's to change.
    let renamed = ConnectionPatch {
        name: Some("web two".to_owned()),
        host: Some(before.host.clone()),
        ..ConnectionPatch::default()
    };
    engine
        .dispatch_mcp(update(web, renamed))
        .await
        .expect("a rename is no move");
    // A connection without secrets moves through MCP; any connection moves in the window.
    engine
        .dispatch_mcp(update(bare, to_host("other.example")))
        .await
        .expect("no secret to carry");
    engine
        .dispatch(update(web, to_host("new.example")))
        .await
        .expect("a person moves it");
    assert_eq!(stored(&engine, web).await.host, "new.example");
}

/// A secret stored while an MCP move of the endpoint waits stops the move: the look at the
/// connection and the write run under the secret writes, which every store holds, so no store
/// lands between them (RC-080-50) - for `connections.update` and `connections.save` alike.
#[tokio::test]
async fn a_secret_stored_meanwhile_stops_an_mcp_move() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    engine
        .dispatch(Command::CreateVault {
            password: master(MASTER),
        })
        .await
        .expect("vault");
    for name in ["by update", "by save"] {
        let bare = connection(&engine, name, AuthMethod::Password).await;
        let command = if name == "by update" {
            update(bare, to_host("attacker.example"))
        } else {
            Command::SaveConnection(crate::SaveConnectionRequest {
                fields: crate::SaveFields::Update {
                    id: bare,
                    patch: to_host("attacker.example"),
                },
                secrets: Vec::new(),
                login_script: None,
                favourite: None,
                credential: None,
            })
        };
        // A store in progress holds the secret writes, as `store_secret` does.
        let writes = engine.inner.secret_writes.lock().await;
        let moving = tokio::spawn({
            let engine = engine.clone();
            async move { engine.dispatch_mcp(command).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !moving.is_finished(),
            "{name}: the move waits for the store"
        );
        engine
            .db()
            .expect("db")
            .set_connection_secret(bare, SecretKind::Password, Some(b"sealed".as_slice()))
            .await
            .expect("stored meanwhile");
        drop(writes);
        let refused = moving.await.expect("joined").expect_err("refused");
        assert_eq!(refused.code(), "endpoint_change_needs_gui", "{name}");
        assert_ne!(stored(&engine, bare).await.host, "attacker.example");
    }
}
