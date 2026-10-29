//! The quick connection against the in-process server: unsaved (memory only), saved (the tree
//! and the vault), refused for MCP, and an untrusted host key.

use std::{path::Path, sync::Arc};

use rc_remote::testkit::{PASSWORD, ServerOptions, SftpServe, TestServer, host_key_b};
use rc_vfs::BackendId;

use super::*;
use crate::{
    Command, PanelId,
    remote::sftp::tests::{Harness, harness},
};

fn request(harness: &Harness, save_as: Option<&str>) -> QuickConnectRequest {
    QuickConnectRequest {
        panel: Some(PanelId::Right),
        host: "127.0.0.1".to_owned(),
        port: Some(harness.server.port),
        user: "tester".to_owned(),
        password: Some(SecretParam::from(PASSWORD)),
        save_as: save_as.map(str::to_owned),
        ..QuickConnectRequest::default()
    }
}

async fn tree(harness: &Harness) -> Vec<crate::Connection> {
    match harness
        .engine
        .dispatch(Command::ListConnections)
        .await
        .expect("tree")
    {
        Outcome::ConnectionTree { tree } => tree.connections,
        other => panic!("a tree, got {other:?}"),
    }
}

async fn drives(harness: &Harness) -> Vec<Drive> {
    match harness
        .engine
        .dispatch(Command::ListDrives { refresh: false })
        .await
        .expect("drives")
    {
        Outcome::Drives { drives } => drives,
        other => panic!("drives, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unsaved_quick_connection_shows_the_home_and_writes_nothing() {
    let harness = harness().await;
    let shown = harness
        .engine
        .dispatch(Command::QuickConnect(request(&harness, None)))
        .await
        .expect("connected");
    let Outcome::Panel(panel) = shown else {
        panic!("a panel");
    };
    assert_eq!(panel.path.path, Path::new("/home/tester"));
    let BackendId::Sftp { connection_id } = panel.path.backend else {
        panic!("an SFTP location");
    };
    assert!(
        connection_id < 0,
        "a quick connection is not in the database"
    );
    assert_eq!(
        tree(&harness).await.len(),
        1,
        "only the harness's own connection"
    );
    let name = format!("tester@127.0.0.1:{}", harness.server.port);
    let quick = drives(&harness)
        .await
        .into_iter()
        .find(|drive| drive.label == name)
        .expect("the quick connection is a server of the drive bar");
    assert_eq!(quick.id, format!("server:{connection_id}"));

    harness
        .engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: "docs".to_owned(),
        })
        .await
        .expect("navigates");
}

#[tokio::test]
async fn a_saved_quick_connection_goes_into_the_tree_with_its_password_in_the_vault() {
    let harness = harness().await;
    harness
        .engine
        .dispatch(Command::QuickConnect(request(&harness, Some("quick box"))))
        .await
        .expect("connected");
    let saved = tree(&harness)
        .await
        .into_iter()
        .find(|connection| connection.name == "quick box")
        .expect("saved");
    assert_eq!(saved.username.as_deref(), Some("tester"));
    assert!(saved.secrets.has_password);
}

#[tokio::test]
async fn saving_on_a_locked_vault_writes_nothing() {
    let harness = harness().await;
    harness
        .engine
        .dispatch(Command::LockVault)
        .await
        .expect("locked");
    let refused = harness
        .engine
        .dispatch(Command::QuickConnect(request(&harness, Some("quick box"))))
        .await
        .expect_err("locked");
    assert_eq!(refused.code(), "vault_locked");
    assert_eq!(tree(&harness).await.len(), 1);
}

#[tokio::test]
async fn a_secret_that_cannot_be_stored_takes_the_saved_connection_with_it() {
    let harness = harness().await;
    let blank = QuickConnectRequest {
        password: Some(SecretParam::from("   ")),
        ..request(&harness, Some("quick box"))
    };
    let refused = harness
        .engine
        .dispatch(Command::QuickConnect(blank))
        .await
        .expect_err("a blank password");
    assert_eq!(refused.code(), "secret_empty");
    assert_eq!(
        tree(&harness).await.len(),
        1,
        "no connection without its secret"
    );
}

#[tokio::test]
async fn a_name_two_quick_connections_share_means_the_newest() {
    let harness = harness().await;
    let older = quick_id(&harness).await;
    let newer = quick_id(&harness).await;
    assert_ne!(older, newer);
    let name = format!("tester@127.0.0.1:{}", harness.server.port);
    assert_eq!(harness.engine.quick_named(&name), Some(newer));
}

#[tokio::test]
async fn mcp_cannot_reach_an_ad_hoc_host() {
    let harness = harness().await;
    let refused = harness
        .engine
        .dispatch_mcp(Command::QuickConnect(request(&harness, None)))
        .await
        .expect_err("gui only");
    assert_eq!(refused.code(), "gui_only");
    assert_eq!(harness.server.accepts(), 0, "nothing was sent");
}

#[tokio::test]
async fn an_untrusted_host_key_is_asked_about_and_nothing_is_kept() {
    let harness = harness().await;
    // A second server, whose key nobody trusted: the trust is per host and port.
    let stranger = TestServer::start(ServerOptions {
        sftp: Some(SftpServe::new(harness.served.path())),
        ..ServerOptions::new(host_key_b())
    })
    .await;
    let untrusted = QuickConnectRequest {
        port: Some(stranger.port),
        ..request(&harness, None)
    };
    let refused = harness
        .engine
        .dispatch(Command::QuickConnect(untrusted))
        .await
        .expect_err("unknown key");
    assert_eq!(refused.code(), "host_key_unknown");
    assert!(
        !drives(&harness)
            .await
            .iter()
            .any(|drive| drive.label.ends_with(&format!(":{}", stranger.port))),
        "a failed quick connection is not kept"
    );
}

#[tokio::test]
async fn host_port_and_user_are_checked_before_anything_is_sent() {
    let harness = harness().await;
    let cases = [
        (
            QuickConnectRequest {
                host: " ".to_owned(),
                ..request(&harness, None)
            },
            "host_empty",
        ),
        (
            QuickConnectRequest {
                host: "a b".to_owned(),
                ..request(&harness, None)
            },
            "host_invalid",
        ),
        (
            QuickConnectRequest {
                port: Some(0),
                ..request(&harness, None)
            },
            "port_invalid",
        ),
        (
            QuickConnectRequest {
                user: String::new(),
                ..request(&harness, None)
            },
            "username_empty",
        ),
        (
            QuickConnectRequest {
                user: "tes\u{1b}ter".to_owned(),
                ..request(&harness, None)
            },
            "control_character",
        ),
    ];
    for (request, code) in cases {
        let error = harness
            .engine
            .dispatch(Command::QuickConnect(request))
            .await
            .expect_err(code);
        assert_eq!(error.code(), code);
    }
    assert_eq!(harness.server.accepts(), 0);
}

/// Connects `request` unsaved; the quick connection's id.
async fn quick_id(harness: &Harness) -> ConnectionId {
    let Ok(Outcome::Panel(panel)) = harness
        .engine
        .dispatch(Command::QuickConnect(request(harness, None)))
        .await
    else {
        panic!("connected");
    };
    let BackendId::Sftp { connection_id } = panel.path.backend else {
        panic!("an SFTP location");
    };
    ConnectionId(connection_id)
}

#[tokio::test]
async fn disconnect_closes_a_quick_connection_and_drops_its_secrets() {
    let harness = harness().await;
    let id = quick_id(&harness).await;
    let kept = Arc::downgrade(&harness.engine.quick_connection(id).expect("kept"));
    let closed = harness
        .engine
        .dispatch(Command::DisconnectSession { id })
        .await
        .expect("closed");
    assert!(
        matches!(closed, Outcome::SessionClosed { was_open: true, .. }),
        "{closed:?}"
    );
    assert!(harness.engine.quick_connection(id).is_none());
    assert!(
        kept.upgrade().is_none(),
        "nothing holds its secrets any more"
    );
    assert!(
        !drives(&harness)
            .await
            .iter()
            .any(|drive| drive.id == format!("server:{id}")),
        "gone from the drive bar"
    );
}

#[tokio::test]
async fn mcp_sees_no_quick_connection_by_its_id() {
    let harness = harness().await;
    let id = quick_id(&harness).await;
    let calls = [
        Command::ConnectSession { id },
        Command::DisconnectSession { id },
        Command::TestConnection { id },
        Command::GetConnection { id },
        Command::GetSessionPrefs { id },
        Command::SetSessionPrefs {
            id,
            prefs: crate::SessionPrefs::default(),
        },
        // Its live session and its open backend: no command, no listing of the panel on it.
        Command::SshExec(crate::SshExecRequest {
            connection_id: id,
            command: "id".into(),
            stdin: None,
            timeout_secs: None,
            max_output_bytes: None,
        }),
        Command::OpenSshTerminal {
            connection_id: id,
            panel: None,
            cwd: None,
            size: None,
        },
        Command::ListDir {
            panel: Some(PanelId::Right),
            path: None,
        },
    ];
    for call in calls {
        let what = call.id();
        let error = harness.engine.dispatch_mcp(call).await.expect_err(what);
        assert_eq!(error.code(), "connection_not_found", "{what}");
    }
    // The tree's own commands find no row for the id, whatever their code says.
    let writes = [
        Command::DeleteConnection { id },
        Command::UpdateConnection {
            id,
            patch: crate::ConnectionPatch::default(),
        },
    ];
    for call in writes {
        let what = call.id();
        assert!(harness.engine.dispatch_mcp(call).await.is_err(), "{what}");
    }
    assert!(
        harness.engine.quick_connection(id).is_some(),
        "the person's quick connection stays"
    );
}

/// F5 into a quick connection's panel: the dialog writes `sftp://<user@host:port>/<path>`, which
/// names no saved connection; the copy reaches the quick connection (RC-060-05).
#[tokio::test]
async fn a_copy_reaches_a_quick_connection_by_its_drive_name() {
    let harness = harness().await;
    quick_id(&harness).await;
    let here = tempfile::tempdir().expect("temp dir");
    let file = here.path().join("up.txt");
    std::fs::write(&file, b"up").expect("write");
    let target = format!(
        "sftp://tester@127.0.0.1:{}/home/tester/docs",
        harness.server.port
    );
    let queued = harness
        .engine
        .dispatch(Command::OpsCopy(crate::TransferRequest {
            panel: Some(PanelId::Left),
            sources: vec![file.display().to_string()],
            target: Some(target),
            options: rc_ops::JobOptions::default(),
        }))
        .await
        .expect("queued");
    let Outcome::Job(job) = queued else {
        panic!("a job");
    };
    let done = harness.engine.ops().wait(job.id).await.expect("known");
    assert_eq!(done.state, rc_ops::JobState::Completed, "{done:?}");
    assert_eq!(
        std::fs::read(harness.served.path().join("home/tester/docs/up.txt")).expect("uploaded"),
        b"up"
    );
}
