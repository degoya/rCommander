//! `drives.connections` (RC-110-38) against the in-process FTP and SFTP servers: an open session
//! is listed with the folder last shown there and announced, a click on its button in the other
//! panel returns there, a disconnect takes it from the list, and a click after it starts where
//! the login starts, as for a saved server.

use rc_remote::ftp::test_server::{self, Options, TestServer as FtpServer};
use rc_vfs::{BackendId, VfsPath};

use crate::{
    AuthMethod, Command, ConnectionDraft, ConnectionId, ConnectionKind, Engine, EngineEvent,
    OpenConnection, Outcome, PanelId, SecretKind, SecretParam, SessionChange,
    remote::sftp::tests::harness,
};

/// The FTP connection `name` to `server`, its password stored.
async fn add_ftp(engine: &Engine, server: &FtpServer, name: &str) -> ConnectionId {
    let draft = ConnectionDraft {
        kind: ConnectionKind::Ftp,
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
async fn click(engine: &Engine, panel: PanelId, id: ConnectionId) {
    engine
        .dispatch(Command::ChangeDrive {
            panel: Some(panel),
            drive: format!("server:{id}"),
        })
        .await
        .expect("the server opens");
}

async fn open_connections(engine: &Engine) -> Vec<OpenConnection> {
    match engine
        .dispatch(Command::ListOpenConnections)
        .await
        .expect("listed")
    {
        Outcome::OpenConnections { connections } => connections,
        other => panic!("open connections, not {other:?}"),
    }
}

fn on_ftp(id: ConnectionId, path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Ftp {
            connection_id: id.0,
        },
        path: path.into(),
    }
}

/// An FTP server whose login starts in `/home/tester`, which holds `docs/`.
async fn ftp_server() -> FtpServer {
    let server = FtpServer::start(Options {
        start: "/home/tester",
        ..Options::default()
    })
    .await;
    std::fs::create_dir_all(server.local("home/tester/docs")).expect("docs");
    server
}

#[tokio::test]
async fn an_open_ftp_session_is_listed_with_its_last_folder_and_announced() {
    let base = harness().await;
    let engine = &base.engine;
    let server = ftp_server().await;
    let id = add_ftp(engine, &server, "files").await;
    assert!(
        open_connections(engine).await.is_empty(),
        "nothing open yet"
    );

    let mut events = engine.subscribe();
    click(engine, PanelId::Right, id).await;
    let announced = std::iter::from_fn(|| events.try_recv().ok()).any(|event| {
        event
            == EngineEvent::Session {
                connection: id,
                change: SessionChange::Connected,
            }
    });
    assert!(announced, "the window hears of the new session");
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: "docs".to_owned(),
        })
        .await
        .expect("into docs");

    assert_eq!(
        open_connections(engine).await,
        vec![OpenConnection {
            drive: format!("server:{id}"),
            connection_id: id,
            name: "files".to_owned(),
            kind: ConnectionKind::Ftp,
            last_dir: Some(on_ftp(id, "/home/tester/docs")),
        }]
    );

    engine
        .dispatch(Command::DisconnectSession { id })
        .await
        .expect("disconnected");
    assert!(open_connections(engine).await.is_empty(), "closed, gone");
}

#[tokio::test]
async fn a_click_returns_to_the_sessions_last_folder_and_after_a_disconnect_logs_in_anew() {
    let base = harness().await;
    let engine = &base.engine;
    let server = ftp_server().await;
    let id = add_ftp(engine, &server, "files").await;
    let local = engine.panel(PanelId::Right).path;
    click(engine, PanelId::Right, id).await;
    engine
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: "docs".to_owned(),
        })
        .await
        .expect("into docs");
    // The right panel goes back to the local folder it started in; the session stays open.
    engine
        .dispatch(Command::HistoryJump(crate::HistoryJumpRequest {
            panel: Some(PanelId::Right),
            index: 0,
        }))
        .await
        .expect("back to the local folder");
    assert_eq!(engine.panel(PanelId::Right).path, local);

    // The left panel was never on the server: its own memory has nothing there.
    click(engine, PanelId::Left, id).await;
    assert_eq!(
        engine.panel(PanelId::Left).path,
        on_ftp(id, "/home/tester/docs"),
        "where the session was last"
    );
    assert_eq!(server.passwords_received(), 1, "no new login");

    engine
        .dispatch(Command::DisconnectSession { id })
        .await
        .expect("disconnected");
    click(engine, PanelId::Right, id).await;
    assert_eq!(
        engine.panel(PanelId::Right).path,
        on_ftp(id, "/home/tester/docs"),
        "the panel's own last folder there, as for any saved server"
    );
    assert_eq!(server.passwords_received(), 2, "logged in anew");
}

#[tokio::test]
async fn an_ssh_session_is_an_open_connection_until_it_closes() {
    let base = harness().await;
    let engine = &base.engine;
    click(engine, PanelId::Left, base.id).await;

    let open = open_connections(engine).await;
    assert_eq!(open.len(), 1, "{open:?}");
    assert_eq!(open[0].drive, format!("server:{}", base.id));
    assert_eq!(open[0].name, "box");
    assert_eq!(open[0].kind, ConnectionKind::Ssh);
    assert!(open[0].last_dir.is_some(), "where the panel is");

    engine
        .dispatch(Command::DisconnectSession { id: base.id })
        .await
        .expect("disconnected");
    assert!(open_connections(engine).await.is_empty(), "closed, gone");
}
