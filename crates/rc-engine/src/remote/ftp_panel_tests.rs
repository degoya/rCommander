//! FTP in a panel through the engine (RC-070-03), against rc-remote's in-process FTP server: the
//! drive bar opens a server where its login starts, F5 copies local to FTP, FTP to local, FTP to
//! SFTP and SFTP to FTP byte for byte through the operations queue, a disconnect drops the
//! session and closes its control connections, an untrusted certificate is asked about before
//! any password leaves, and an MCP call on a locked vault opens no dialog.

use rc_ops::{JobOptions, JobSnapshot, JobState};
use rc_remote::ftp::test_server::{self, Options, ServerTls, TestServer};
use rc_vfs::VfsPath;

use super::*;
use crate::{
    Command, ConnectionDraft, Outcome, PanelId, SecretParam, TransferRequest,
    remote::sftp::tests::harness,
};

/// The FTP connection `name` of `kind` to `server`, its password stored.
pub(super) async fn add_ftp(
    engine: &Engine,
    server: &TestServer,
    kind: ConnectionKind,
    name: &str,
) -> ConnectionId {
    let draft = ConnectionDraft {
        kind,
        port: Some(i64::from(server.port)),
        username: Some("tester".to_owned()),
        auth_method: AuthMethod::Password,
        ..ConnectionDraft::new(name, "127.0.0.1")
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
                value: SecretParam::from(test_server::PASSWORD),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }))
        .await
        .expect("password stored");
    connection.id
}

/// A click on the server's drive button in `panel`.
pub(super) async fn open_in(
    engine: &Engine,
    panel: PanelId,
    id: ConnectionId,
) -> Result<Outcome, EngineError> {
    engine
        .dispatch(Command::ChangeDrive {
            panel: Some(panel),
            drive: format!("server:{id}"),
        })
        .await
}

/// F5 on `name` in `panel`, into the other panel; the job once it ended.
pub(super) async fn copy(engine: &Engine, panel: PanelId, name: &str) -> JobSnapshot {
    let queued = engine
        .dispatch(Command::OpsCopy(TransferRequest {
            panel: Some(panel),
            sources: vec![name.to_owned()],
            target: None,
            options: JobOptions::default(),
        }))
        .await
        .expect("queued");
    let Outcome::Job(job) = queued else {
        panic!("a job");
    };
    engine.ops().wait(job.id).await.expect("known")
}

/// `size` bytes no two neighbouring blocks of which are alike.
fn content(size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| u8::try_from(index % 251).expect("below 251"))
        .collect()
}

#[tokio::test]
async fn the_drive_bar_opens_an_ftp_server_and_f5_copies_both_ways() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options {
        start: "/home/tester",
        ..Options::default()
    })
    .await;
    std::fs::create_dir_all(server.local("home/tester/docs")).expect("home");
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "files").await;

    let Outcome::Panel(panel) = open_in(engine, PanelId::Right, id)
        .await
        .expect("the server opens")
    else {
        panic!("a panel");
    };
    assert_eq!(
        panel.path,
        VfsPath {
            backend: backend(id),
            path: "/home/tester".into(),
        },
        "where the login starts"
    );
    assert_eq!(server.passwords_received(), 1);

    let local = engine.panel(PanelId::Left).path.path;
    let data = content(300 * 1024);
    std::fs::write(local.join("data.bin"), &data).expect("local file");
    let done = copy(engine, PanelId::Left, "data.bin").await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let uploaded = std::fs::read(server.local("home/tester/data.bin")).expect("uploaded");
    assert!(uploaded == data, "local to FTP, byte for byte");

    std::fs::create_dir(local.join("back")).expect("folder");
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: "back".to_owned(),
        })
        .await
        .expect("into back");
    let done = copy(engine, PanelId::Right, "data.bin").await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let downloaded = std::fs::read(local.join("back/data.bin")).expect("downloaded");
    assert!(downloaded == data, "FTP to local, byte for byte");
}

#[tokio::test]
async fn f5_copies_between_ftp_and_sftp() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options::default()).await;
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "files").await;
    open_in(engine, PanelId::Left, base.id)
        .await
        .expect("SFTP opens");
    open_in(engine, PanelId::Right, id)
        .await
        .expect("FTP opens");

    let done = copy(engine, PanelId::Left, "readme.txt").await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(
        std::fs::read(server.local("readme.txt")).expect("on the FTP server"),
        b"hello",
        "SFTP to FTP"
    );

    let data = content(200 * 1024);
    std::fs::write(server.local("big.bin"), &data).expect("FTP file");
    let done = copy(engine, PanelId::Right, "big.bin").await;
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    let copied = std::fs::read(base.served.path().join("home/tester/big.bin")).expect("copied");
    assert!(copied == data, "FTP to SFTP, byte for byte");
}

#[tokio::test]
async fn a_disconnect_drops_the_session_and_the_next_call_logs_in_again() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options::default()).await;
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "files").await;
    let local = engine.panel(PanelId::Right).path;
    open_in(engine, PanelId::Right, id)
        .await
        .expect("FTP opens");
    assert_eq!(server.passwords_received(), 1);

    let closed = engine
        .dispatch(Command::DisconnectSession { id })
        .await
        .expect("disconnected");
    assert!(
        matches!(closed, Outcome::SessionClosed { was_open: true, .. }),
        "{closed:?}"
    );
    assert!(engine.inner.remotes.ftp().is_empty(), "the session is gone");
    assert_eq!(
        engine.panel(PanelId::Right).path,
        local,
        "the panel is back on its local folder"
    );

    open_in(engine, PanelId::Right, id)
        .await
        .expect("FTP opens again");
    assert_eq!(server.passwords_received(), 2, "a new login");
}

/// Waits until the server has `count` control connections open, for at most five seconds.
async fn sessions_reach(server: &TestServer, count: usize) {
    for _ in 0..500 {
        if server.open_sessions() == count {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!(
        "{} control connections open, not {count}",
        server.open_sessions()
    );
}

/// Ctrl+Shift+F on an FTP panel (the owner, 2026-10-18; Total Commander's behaviour): every
/// control connection of the session says goodbye at once - also while something still holds
/// the backend, as a running job or a viewer does - the end is announced as an SSH session's
/// is, and both panels showing the server go back to the local folder they showed before it.
#[tokio::test]
async fn a_disconnect_closes_every_control_connection_and_both_panels_leave_the_server() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options::default()).await;
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "files").await;
    let local = [PanelId::Left, PanelId::Right].map(|panel| engine.panel(panel).path);
    for panel in [PanelId::Left, PanelId::Right] {
        open_in(engine, panel, id).await.expect("FTP opens");
    }
    assert!(server.open_sessions() >= 1, "the session is open");
    let held = engine
        .inner
        .remotes
        .ftp()
        .get(&id.0)
        .cloned()
        .expect("the kept session");
    let mut events = engine.subscribe();

    engine
        .dispatch(Command::DisconnectSession { id })
        .await
        .expect("disconnected");
    sessions_reach(&server, 0).await;
    assert!(engine.inner.remotes.ftp().is_empty(), "nothing is kept");
    let announced = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        event
            == crate::EngineEvent::Session {
                connection: id,
                change: crate::SessionChange::Closed,
            }
    });
    assert!(announced, "the session's end is announced");
    assert_eq!(
        [PanelId::Left, PanelId::Right].map(|panel| engine.panel(panel).path),
        local,
        "both panels left the server"
    );
    drop(held);
}

#[tokio::test]
async fn an_untrusted_certificate_is_asked_about_before_any_password_leaves() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options {
        tls: ServerTls::Explicit,
        ..Options::default()
    })
    .await;
    let id = add_ftp(engine, &server, ConnectionKind::Ftps, "secure").await;
    let refused = open_in(engine, PanelId::Right, id)
        .await
        .expect_err("not trusted");
    assert_eq!(refused.code(), "certificate_unknown");
    assert_eq!(server.passwords_received(), 0, "no password before trust");
}

#[tokio::test]
async fn an_mcp_call_on_a_locked_vault_answers_vault_locked() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options::default()).await;
    add_ftp(engine, &server, ConnectionKind::Ftp, "files").await;
    engine.dispatch(Command::LockVault).await.expect("locked");
    for location in ["ftp://files/", "ftps://files"] {
        let refused = engine
            .dispatch_mcp(Command::ListDir {
                panel: None,
                path: Some(location.to_owned()),
            })
            .await
            .expect_err("locked");
        assert_eq!(refused.code(), "vault_locked", "{location}");
    }
    assert_eq!(server.passwords_received(), 0);
}

/// `ops.copy` reaches an FTP server written as `ftp://<connection>/<path>` with no panel on it:
/// the queue gets its backend before the job is submitted.
#[tokio::test]
async fn ops_copy_reaches_an_ftp_server_by_its_location() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options::default()).await;
    std::fs::create_dir_all(server.local("in")).expect("folder");
    add_ftp(engine, &server, ConnectionKind::Ftp, "files").await;
    let here = tempfile::tempdir().expect("temp dir");
    let file = here.path().join("up.txt");
    std::fs::write(&file, b"up").expect("write");

    let queued = engine
        .dispatch(Command::OpsCopy(TransferRequest {
            panel: None,
            sources: vec![file.display().to_string()],
            target: Some("ftp://files/in".to_owned()),
            options: JobOptions::default(),
        }))
        .await
        .expect("queued");
    let Outcome::Job(job) = queued else {
        panic!("a job");
    };
    let done = engine.ops().wait(job.id).await.expect("known");
    assert_eq!(done.state, JobState::Completed, "{done:?}");
    assert_eq!(
        std::fs::read(server.local("in/up.txt")).expect("uploaded"),
        b"up"
    );
}

/// An FTP connection to `server` that opens in `start`, its password stored.
async fn add_ftp_at(engine: &Engine, server: &TestServer, start: &str) -> ConnectionId {
    let draft = ConnectionDraft {
        kind: ConnectionKind::Ftp,
        port: Some(i64::from(server.port)),
        username: Some("tester".to_owned()),
        auth_method: AuthMethod::Password,
        start_directory: Some(start.to_owned()),
        ..ConnectionDraft::new("started", "127.0.0.1")
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
                value: SecretParam::from(test_server::PASSWORD),
            }],
            login_script: None,
            favourite: None,
            credential: None,
        }))
        .await
        .expect("password stored");
    connection.id
}

#[tokio::test]
async fn a_server_that_refuses_epsv_opens_with_pasv() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options {
        start: "/home/tester",
        refuse: &["EPSV"],
        ..Options::default()
    })
    .await;
    std::fs::create_dir_all(server.local("home/tester/docs")).expect("home");
    std::fs::create_dir_all(server.local("srv")).expect("srv");
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "files").await;
    let Outcome::Panel(panel) = open_in(engine, PanelId::Right, id)
        .await
        .expect("the server opens where its login starts")
    else {
        panic!("a panel");
    };
    assert_eq!(panel.path.path, std::path::Path::new("/home/tester"));

    let id = add_ftp_at(engine, &server, "/srv").await;
    let Outcome::Panel(panel) = open_in(engine, PanelId::Left, id)
        .await
        .expect("the server opens in its start directory")
    else {
        panic!("a panel");
    };
    assert_eq!(panel.path.path, std::path::Path::new("/srv"));
}

#[tokio::test]
async fn a_server_without_passive_mode_names_it_though_its_test_passes() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options {
        refuse: &["EPSV", "PASV"],
        ..Options::default()
    })
    .await;
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "files").await;
    engine
        .dispatch(Command::TestConnection { id })
        .await
        .expect("the login works");
    let error = open_in(engine, PanelId::Right, id)
        .await
        .expect_err("no data connection");
    assert_eq!(error.code(), "ftp_unsupported_passive");
}

/// RC-080-12 (X7): a session to a plain-FTP server tells the window that the password and the
/// data cross the network in clear.
#[tokio::test]
async fn a_plain_ftp_session_warns_the_window() {
    let base = harness().await;
    let engine = &base.engine;
    let server = TestServer::start(Options {
        start: "/home/tester",
        ..Options::default()
    })
    .await;
    std::fs::create_dir_all(server.local("home/tester")).expect("home");
    let id = add_ftp(engine, &server, ConnectionKind::Ftp, "plain").await;
    let mut events = engine.subscribe();
    open_in(engine, PanelId::Right, id)
        .await
        .expect("the server opens");
    let warned = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        matches!(&event, crate::EngineEvent::PlainFtp { connection, name }
            if *connection == id && name == "plain")
    });
    assert!(warned, "the window is told");
}
