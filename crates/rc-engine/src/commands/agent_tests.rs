use std::time::Duration;

use rc_db::Db;
use rc_remote::{AgentIdentity, AgentSourceKind};
use rc_vault::{ManualClock, MasterPassword, Vault, VaultStateKind};

use super::*;
use crate::Command;
use crate::test_support;
use crate::test_support::engine;

/// A vault with a master password, locked: what a restart leaves.
async fn locked_vault() -> Vault {
    let db = Db::open_in_memory().await.expect("db");
    let vault = Vault::open_with(
        db,
        test_support::fast_vault_options(&ManualClock::new(), &[]),
    )
    .await
    .expect("vault");
    vault
        .create(&MasterPassword::new("pw".to_owned()))
        .await
        .expect("create");
    assert!(vault.lock(), "was unlocked");
    vault
}

#[tokio::test]
async fn without_an_agent_the_answer_is_agent_not_found_at_once() {
    let (root, engine) = engine(&[]).await;
    assert!(
        engine.attach_agent_sources(vec![AgentSource::UnixSocket(root.path().join("none.sock"))])
    );
    let started = std::time::Instant::now();
    let error = engine
        .dispatch_mcp(Command::ListAgentIdentities)
        .await
        .expect_err("no agent");
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(error.code(), "agent_not_found");
    assert!(
        error.to_string().starts_with("no SSH agent found"),
        "{error}"
    );
}

#[tokio::test]
async fn no_configured_agent_says_why() {
    let (_root, engine) = engine(&[]).await;
    assert!(engine.attach_agent_sources(Vec::new()));
    assert!(!engine.attach_agent_sources(Vec::new()), "fixed once");
    let error = engine
        .dispatch(Command::ListAgentIdentities)
        .await
        .expect_err("nothing to ask");
    assert_eq!(error.code(), "agent_not_found");
    assert!(error.to_string().contains("SSH_AUTH_SOCK"), "{error}");
}

/// The agent path needs no secret: with a locked vault the listing neither fails on the vault
/// nor unlocks it, and announces nothing a GUI would answer with the master-password dialog.
#[tokio::test]
async fn listing_the_agent_never_touches_the_vault() {
    let (root, engine) = engine(&[]).await;
    engine.attach_vault(locked_vault().await);
    engine.attach_agent_sources(vec![AgentSource::UnixSocket(root.path().join("none.sock"))]);
    let mut events = engine.subscribe();
    let error = engine
        .dispatch(Command::ListAgentIdentities)
        .await
        .expect_err("no agent");
    assert_ne!(error.code(), "vault_locked");
    assert_eq!(
        engine.vault().expect("attached").status().state,
        VaultStateKind::Locked
    );
    assert!(events.try_recv().is_err(), "nothing announced");
}

#[tokio::test]
async fn the_setting_reads_as_its_names_and_defaults_to_both() {
    let (_root, engine) = engine(&[]).await;
    assert_eq!(
        engine.windows_agents().await.expect("default"),
        WindowsAgents::Both
    );
    let db = Db::open_in_memory().await.expect("db");
    db.set(AGENT_SOURCES_KEY, &serde_json::json!("pageant"))
        .await
        .expect("set");
    engine.attach_db(db);
    assert_eq!(
        engine.windows_agents().await.expect("stored"),
        WindowsAgents::Pageant
    );
}

#[test]
fn the_outcome_names_type_comment_fingerprint_and_agent_only() {
    let outcome = Outcome::AgentIdentities {
        sources: vec![AgentSourceKind::OpensshPipe, AgentSourceKind::Pageant],
        identities: vec![AgentIdentity {
            source: AgentSourceKind::Pageant,
            algorithm: "ssh-ed25519".to_owned(),
            fingerprint_sha256: "SHA256:5RiEvVR9s2eAmq1pXd0zFRd1zhBaDjgSOeBsI9EpDw0".to_owned(),
            comment: "me@box".to_owned(),
        }],
    };
    let value = serde_json::to_value(&outcome).expect("serializes");
    assert_eq!(value["kind"], "agent_identities");
    assert_eq!(
        value["sources"],
        serde_json::json!(["openssh_pipe", "pageant"])
    );
    let mut fields: Vec<&str> = value["identities"][0]
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    fields.sort_unstable();
    assert_eq!(
        fields,
        ["algorithm", "comment", "fingerprint_sha256", "source"]
    );
}
