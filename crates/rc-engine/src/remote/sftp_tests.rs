//! SFTP through the engine against the in-process server: `sftp://` locations, the drive bar,
//! the panel, the operations queue, a lost session and an MCP call on a locked vault.

use std::{path::Path, time::Duration};

use rc_db::Db;
use rc_ops::{DeleteMode, JobState};
use rc_remote::{
    OfferedKey,
    testkit::{PASSWORD, ServerOptions, SftpServe, TestServer, host_key_a},
    trust,
};
use rc_vfs::{BackendId, VfsPath};

use super::*;
use crate::test_support;
use crate::{
    Command, ConnectionDraft, DeleteRequest, EngineEvent, Outcome, PanelId, SecretKind,
    SecretParam, SessionChange, SessionPrefs,
};

pub(crate) struct Harness {
    _local: tempfile::TempDir,
    pub served: tempfile::TempDir,
    pub engine: Engine,
    pub server: TestServer,
    pub id: ConnectionId,
}

/// An engine with an unlocked vault and the SSH connection `box` to a server that serves
/// `served` over SFTP, with `/home/tester` as home; its key is trusted, its password stored.
pub(crate) async fn harness() -> Harness {
    let local = tempfile::tempdir().expect("temp dir");
    let served = tempfile::tempdir().expect("served");
    std::fs::create_dir_all(served.path().join("home/tester/docs")).expect("home");
    std::fs::write(served.path().join("home/tester/readme.txt"), b"hello").expect("file");
    let db = Db::open_in_memory().await.expect("db");
    let engine = test_support::headless(local.path()).await;
    engine.attach_db(db.clone());
    engine.attach_agent_sources(Vec::new());
    let vault = test_support::fast_vault(db.clone()).await;
    engine.attach_vault(vault);
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from("correct horse battery"),
        })
        .await
        .expect("vault created");
    let mut serve = SftpServe::new(served.path());
    serve.home = "/home/tester".to_owned();
    let server = TestServer::start(ServerOptions {
        sftp: Some(serve),
        ..ServerOptions::new(host_key_a())
    })
    .await;
    let draft = ConnectionDraft {
        port: Some(i64::from(server.port)),
        username: Some("tester".to_owned()),
        ..ConnectionDraft::new("box", "127.0.0.1")
    };
    let Ok(Outcome::Connection { connection }) = engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
    else {
        panic!("a connection");
    };
    engine
        .dispatch(Command::SaveConnection(crate::SaveConnectionRequest {
            fields: crate::SaveFields::Update {
                id: connection.id,
                patch: crate::ConnectionPatch::default(),
            },
            secrets: vec![crate::SecretChange::Set {
                kind: SecretKind::Password,
                value: SecretParam::from(PASSWORD),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }))
        .await
        .expect("password stored");
    let offered = OfferedKey::from_public_key(host_key_a().public_key()).expect("a key");
    trust::trust(&db, "127.0.0.1", server.port, offered)
        .await
        .expect("trusted");
    Harness {
        _local: local,
        served,
        engine,
        server,
        id: connection.id,
    }
}

fn names(outcome: Outcome) -> Vec<String> {
    match outcome {
        Outcome::Listing { entries, .. } => entries.into_iter().map(|entry| entry.name).collect(),
        other => panic!("a listing, got {other:?}"),
    }
}

#[test]
fn a_location_is_written_with_the_connection_s_name() {
    assert_eq!(sftp_location("box", Path::new("/")), "sftp://box/");
    assert_eq!(
        sftp_location("box", Path::new("/home/tester/My Files")),
        "sftp://box/home/tester/My Files"
    );
    assert_eq!(strip_sftp_scheme("SFTP://box/x"), Some("box/x"));
    assert_eq!(strip_sftp_scheme("ftp://box/x"), None);
}

#[test]
fn an_sftp_failure_keeps_its_code_also_inside_a_file_system_error() {
    let direct = EngineError::from(rc_remote::SftpError::Unavailable);
    assert_eq!(direct.code(), "sftp_unavailable");
    let inside = EngineError::from(rc_vfs::VfsError::Io {
        path: "/srv".into(),
        source: std::io::Error::new(
            std::io::ErrorKind::ConnectionAborted,
            rc_remote::SftpError::ConnectionLost,
        ),
    });
    assert_eq!(inside.code(), "connection_lost");
    for code in rc_remote::SftpError::ALL_CODES {
        assert!(!EngineError::ALL_CODES.contains(code), "{code} is SFTP's");
    }
}

#[tokio::test]
async fn list_dir_and_stat_read_an_sftp_location_like_a_local_one() {
    let harness = harness().await;
    let listing = harness
        .engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some("sftp://box".to_owned()),
        })
        .await
        .expect("the home directory");
    let Outcome::Listing { path, .. } = &listing else {
        panic!("a listing");
    };
    assert_eq!(
        path,
        &VfsPath {
            backend: BackendId::Sftp {
                connection_id: harness.id.0,
            },
            path: "/home/tester".into(),
        }
    );
    assert_eq!(names(listing), ["docs", "readme.txt"]);

    let stat = harness
        .engine
        .dispatch(Command::Stat {
            panel: None,
            path: "sftp://box/home/tester/readme.txt".to_owned(),
        })
        .await
        .expect("stat");
    let Outcome::Stat { entry, .. } = stat else {
        panic!("a stat");
    };
    assert_eq!((entry.name.as_str(), entry.size), ("readme.txt", 5));

    let unknown = harness
        .engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some("sftp://nowhere/".to_owned()),
        })
        .await
        .expect_err("no such connection");
    assert_eq!(unknown.code(), "sftp_connection_not_found");
    assert_eq!(
        harness.server.accepts(),
        1,
        "one session carries every call"
    );
}

#[tokio::test]
async fn the_drive_bar_opens_the_server_in_a_panel_where_the_queue_reaches_it() {
    let harness = harness().await;
    let shown = harness
        .engine
        .dispatch(Command::ChangeDrive {
            panel: Some(PanelId::Left),
            drive: format!("server:{}", harness.id),
        })
        .await
        .expect("the server opens");
    let Outcome::Panel(panel) = shown else {
        panic!("a panel");
    };
    assert_eq!(panel.path.path, Path::new("/home/tester"));
    harness
        .engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: "docs".to_owned(),
        })
        .await
        .expect("enter docs");

    let made = harness
        .engine
        .dispatch(Command::OpsMkdir {
            panel: Some(PanelId::Left),
            path: "new".to_owned(),
        })
        .await
        .expect("queued");
    let Outcome::Job(job) = made else {
        panic!("a job");
    };
    let done = harness.engine.ops().wait(job.id).await.expect("known");
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(harness.served.path().join("home/tester/docs/new").is_dir());

    let removed = harness
        .engine
        .dispatch(Command::OpsDelete(DeleteRequest {
            panel: Some(PanelId::Left),
            sources: vec!["new".to_owned()],
            mode: DeleteMode::Permanent,
            on_error: rc_ops::ErrorPolicy::default(),
            queue: None,
        }))
        .await
        .expect("queued");
    let Outcome::Job(job) = removed else {
        panic!("a job");
    };
    let done = harness.engine.ops().wait(job.id).await.expect("known");
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert!(!harness.served.path().join("home/tester/docs/new").exists());
}

/// Ctrl+Shift+F on an SFTP panel acts as on an FTP one (owner, 2026-10-18): the session closes,
/// both panels showing the server go back to the local folder they showed before it, and going
/// to the server again connects again.
#[tokio::test]
async fn a_disconnect_closes_the_session_and_both_panels_leave_the_server() {
    let harness = harness().await;
    let engine = &harness.engine;
    let local = [PanelId::Left, PanelId::Right].map(|panel| engine.panel(panel).path);
    for panel in [PanelId::Left, PanelId::Right] {
        engine
            .dispatch(Command::ChangeDrive {
                panel: Some(panel),
                drive: format!("server:{}", harness.id),
            })
            .await
            .expect("the server opens");
    }
    let mut events = engine.subscribe();
    engine
        .dispatch(Command::DisconnectSession { id: harness.id })
        .await
        .expect("disconnected");
    assert_eq!(
        [PanelId::Left, PanelId::Right].map(|panel| engine.panel(panel).path),
        local,
        "both panels left the server"
    );
    assert!(engine.ssh_session(harness.id).is_none(), "no session kept");
    let closed = crate::EngineEvent::Session {
        connection: harness.id,
        change: crate::SessionChange::Closed,
    };
    let announced = tokio::time::timeout(Duration::from_secs(5), async {
        while let Ok(event) = events.recv().await {
            if event == closed {
                return true;
            }
        }
        false
    })
    .await;
    assert!(
        matches!(announced, Ok(true)),
        "the session's end is announced"
    );

    engine
        .dispatch(Command::ChangeDrive {
            panel: Some(PanelId::Left),
            drive: format!("server:{}", harness.id),
        })
        .await
        .expect("the server opens again");
    assert!(engine.ssh_session(harness.id).is_some(), "a new session");
}

#[tokio::test]
async fn a_lost_session_is_an_error_and_the_next_call_connects_again() {
    let harness = harness().await;
    let list = || {
        harness.engine.dispatch(Command::ListDir {
            panel: None,
            path: Some("sftp://box/home/tester".to_owned()),
        })
    };
    list().await.expect("first listing");
    harness
        .engine
        .dispatch(Command::DisconnectSession { id: harness.id })
        .await
        .expect("disconnected");
    // The kept backend finds its channels closed: an error with a code, no panic.
    let lost = list().await;
    if let Err(error) = &lost {
        assert_eq!(error.code(), "connection_lost", "{error}");
    }
    let again = list().await.expect("a new session");
    assert_eq!(names(again), ["docs", "readme.txt"]);
}

#[tokio::test]
async fn an_mcp_call_on_a_locked_vault_answers_vault_locked() {
    let harness = harness().await;
    harness
        .engine
        .dispatch(Command::LockVault)
        .await
        .expect("locked");
    let refused = harness
        .engine
        .dispatch_mcp(Command::ListDir {
            panel: None,
            path: Some("sftp://box/home/tester".to_owned()),
        })
        .await
        .expect_err("locked");
    assert_eq!(refused.code(), "vault_locked");
}

#[tokio::test]
async fn an_unused_sftp_backend_lets_its_session_close_after_the_idle_time() {
    let harness = harness().await;
    harness
        .engine
        .dispatch(Command::SetSessionPrefs {
            id: harness.id,
            prefs: SessionPrefs {
                idle_close_secs: 1,
                ..SessionPrefs::default()
            },
        })
        .await
        .expect("idle time set");
    let mut events = harness.engine.subscribe();
    let opened = harness
        .engine
        .open_sftp(harness.id, true)
        .await
        .expect("SFTP");
    // While a call holds the backend, its channels keep the session open past the idle time.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let channels = harness.engine.ssh_session(harness.id).map(|s| s.channels);
    assert_eq!(channels, Some(2));
    drop(opened);
    let closed = tokio::time::timeout(Duration::from_secs(6), async {
        loop {
            if let Ok(EngineEvent::Session {
                change: SessionChange::Closed,
                ..
            }) = events.recv().await
            {
                return;
            }
        }
    })
    .await;
    assert!(closed.is_ok(), "the session closes once SFTP is unused");
    assert!(
        harness.engine.inner.remotes.sftp().is_empty(),
        "the backend went"
    );
    let again = harness
        .engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some("sftp://box/home/tester".to_owned()),
        })
        .await
        .expect("the next use opens it again");
    assert_eq!(names(again), ["docs", "readme.txt"]);
}
