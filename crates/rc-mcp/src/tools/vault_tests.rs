use std::{sync::Arc, time::Duration};

use rc_engine::Engine;
use rc_vault::{KdfParams, ManualClock, MasterPassword, Vault, VaultOptions};
use rmcp::model::CallToolResult;
use serde_json::{Value, json};

use crate::RcMcpServer;
use crate::test_support::Fixture;

fn body(result: &CallToolResult) -> Value {
    let text = result
        .content
        .first()
        .and_then(|content| content.as_text())
        .map(|text| text.text.clone())
        .expect("text");
    serde_json::from_str(&text).expect("json")
}

async fn server(with_vault: bool) -> (tempfile::TempDir, RcMcpServer, Engine) {
    let built = Fixture::new().build().await;
    if with_vault {
        let vault = Vault::open_with(
            built.db.clone(),
            VaultOptions {
                clock: Arc::new(ManualClock::new()),
                kdf: KdfParams {
                    m_cost_kib: 64,
                    t_cost: 1,
                    p_cost: 1,
                },
                failure_delay: Duration::ZERO,
                secret_columns: &[],
            },
        )
        .await
        .expect("vault");
        vault
            .create(&MasterPassword::new("pw".to_owned()))
            .await
            .expect("create");
        built.engine.attach_vault(vault);
    }
    let server = built.server();
    (built.root, server, built.engine)
}

#[tokio::test]
async fn without_a_vault_the_status_is_absent() {
    let (_root, server, _engine) = server(false).await;
    let result = server.vault_status().await.expect("answer");
    assert_ne!(result.is_error, Some(true));
    assert_eq!(
        body(&result),
        json!({ "state": "absent", "idle_lock_minutes": 15 })
    );
}

#[tokio::test]
async fn status_and_lock_report_the_state_and_nothing_secret() {
    let (_root, server, engine) = server(true).await;
    let status = body(&server.vault_status().await.expect("answer"));
    assert_eq!(status["state"], "unlocked");
    assert_eq!(status["locks_in_secs"], 15 * 60);
    let mut keys: Vec<&str> = status
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, ["idle_lock_minutes", "locks_in_secs", "state"]);

    let locked = body(&server.vault_lock().await.expect("answer"));
    assert_eq!(
        locked,
        json!({ "state": "locked", "idle_lock_minutes": 15 })
    );
    let vault = engine.vault().expect("attached");
    assert!(!vault.lock(), "the tool locked it");
}

#[test]
fn no_tool_unlocks_the_vault() {
    let names = RcMcpServer::tool_names();
    assert!(names.contains(&"vault_status".to_owned()));
    assert!(names.contains(&"vault_lock".to_owned()));
    assert!(
        names.iter().all(|name| {
            !["unlock", "password", "secret", "master", "key_import"]
                .iter()
                .any(|part| name.contains(part))
        }),
        "{names:?}"
    );
}

/// What a tool that needs a stored secret answers while the vault is locked: the stable code
/// and where the person unlocks it - never a way to unlock it over MCP.
#[test]
fn a_locked_vault_answers_vault_locked_with_the_way_out() {
    let answer =
        crate::answer::respond::<()>(Err(rc_engine::EngineError::VaultLocked)).expect("an answer");
    assert_eq!(answer.is_error, Some(true));
    let body = body(&answer);
    assert_eq!(body["code"], "vault_locked");
    assert!(
        body["error"]
            .as_str()
            .is_some_and(|message| message.contains("rCommander window")),
        "{body}"
    );
}
