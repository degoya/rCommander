//! FTP connections as file systems, against a scripted plain-FTP server that knows just enough
//! to log in. The FTP protocol itself is `rc-remote`'s (`ftp/vfs_tests.rs`, a real server).

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use rc_vfs::{BackendId, VfsPath};
use tokio::{
    io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
    net::TcpListener,
};

use crate::test_support;
use crate::test_support::engine_in;
use crate::{
    AuthMethod, Command, ConnectionDraft, ConnectionId, ConnectionKind, ConnectionPatch, Engine,
    EngineEvent, EntryKind, FolderId, Outcome, SecretKind, SecretParam,
};

const MASTER: &str = "correct horse battery";
const PASSWORD: &str = "hunter2-ftp";
/// What `rc-remote` sends as the password of an anonymous login.
const ANONYMOUS_PASSWORD: &str = "anonymous@example.invalid";

/// A plain FTP server on 127.0.0.1 that answers `USER`, `PASS` (230 for `password`, else 530)
/// and `TYPE`, and `502` to everything else - `AUTH TLS` included; it records every `PASS`
/// argument, and every command line in the second log.
async fn fake_server(password: &'static str) -> (u16, Arc<Mutex<Vec<String>>>) {
    let (port, seen, _) = recording_server(password).await;
    (port, seen)
}

type Log = Arc<Mutex<Vec<String>>>;

async fn recording_server(password: &'static str) -> (u16, Log, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let port = listener.local_addr().expect("address").port();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let commands = Arc::new(Mutex::new(Vec::new()));
    let (log, all) = (Arc::clone(&seen), Arc::clone(&commands));
    tokio::spawn(async move {
        while let Ok((socket, _)) = listener.accept().await {
            let (log, all) = (Arc::clone(&log), Arc::clone(&all));
            tokio::spawn(async move {
                let (read, mut write) = socket.into_split();
                let mut lines = BufReader::new(read).lines();
                if write.write_all(b"220 ready\r\n").await.is_err() {
                    return;
                }
                while let Ok(Some(line)) = lines.next_line().await {
                    all.lock().expect("log").push(line.clone());
                    let (verb, argument) = line.split_once(' ').unwrap_or((line.as_str(), ""));
                    let reply = match verb.to_ascii_uppercase().as_str() {
                        "USER" => "331 password, please",
                        "PASS" => {
                            log.lock().expect("log").push(argument.to_owned());
                            if argument == password {
                                "230 logged in"
                            } else {
                                "530 login incorrect"
                            }
                        }
                        "TYPE" => "200 binary",
                        "QUIT" => "221 bye",
                        _ => "502 not implemented",
                    };
                    if write
                        .write_all(format!("{reply}\r\n").as_bytes())
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            });
        }
    });
    (port, seen, commands)
}

/// A port nothing listens on.
async fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    listener.local_addr().expect("address").port()
}

async fn create(engine: &Engine, folder: Option<FolderId>, draft: ConnectionDraft) -> ConnectionId {
    match engine
        .dispatch(Command::CreateConnection {
            folder_id: folder,
            connection: draft,
        })
        .await
        .expect("connection")
    {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    }
}

fn ftp(name: &str, port: u16, auth_method: AuthMethod) -> ConnectionDraft {
    ConnectionDraft {
        kind: ConnectionKind::Ftp,
        port: Some(i64::from(port)),
        username: Some("web".into()),
        auth_method,
        ..ConnectionDraft::new(name, "127.0.0.1")
    }
}

fn code(result: Result<Outcome, crate::EngineError>) -> &'static str {
    result.expect_err("refused").code()
}

#[tokio::test]
async fn an_ftp_location_names_its_connection() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let nas = create(&engine, None, ftp("NAS", 21, AuthMethod::Anonymous)).await;
    let current = VfsPath::local(dir.path());

    let located = engine
        .locate(&current, "ftp://nas/pub/../incoming")
        .await
        .expect("located");
    assert_eq!(
        located.backend,
        BackendId::Ftp {
            connection_id: nas.0
        }
    );
    assert_eq!(located.path, PathBuf::from("/incoming"));

    // `ftps://` names the same connection; its encryption is the connection's.
    let secure = engine
        .locate(&current, "FTPS://nas/pub")
        .await
        .expect("ftps");
    assert_eq!(secure.backend, located.backend);
    assert_eq!(secure.path, PathBuf::from("/pub"));

    // Without a path: the start directory; without one, where the login starts
    // (`remote/ftp_panel_tests.rs`, against a server).
    engine
        .dispatch(Command::UpdateConnection {
            id: nas,
            patch: ConnectionPatch {
                start_directory: Some("/srv/www".into()),
                ..ConnectionPatch::default()
            },
        })
        .await
        .expect("update");
    let start = engine.locate(&current, "ftp://nas").await.expect("start");
    assert_eq!(start.path, PathBuf::from("/srv/www"));

    // Anything else stays a path of the panel's backend.
    let local = engine.locate(&current, "sub").await.expect("local");
    assert_eq!(local, current.join("sub"));
}

#[tokio::test]
async fn a_location_needs_exactly_one_ftp_connection_of_its_name() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let current = VfsPath::local(dir.path());
    assert_eq!(
        code(
            engine
                .dispatch(Command::ListDir {
                    panel: None,
                    path: Some("ftp://nowhere/".into()),
                })
                .await
        ),
        "ftp_connection_not_found"
    );
    // An SSH connection of that name is not an FTP connection.
    create(&engine, None, ConnectionDraft::new("box", "example.org")).await;
    let refused = engine.locate(&current, "ftp://box/").await;
    assert_eq!(refused.expect_err("ssh").code(), "ftp_connection_not_found");
    // Two FTP connections named alike in two folders cannot be told apart by name.
    let folder = match engine
        .dispatch(Command::CreateFolder {
            parent_id: None,
            name: "old".into(),
        })
        .await
        .expect("folder")
    {
        Outcome::Folder { folder } => folder.id,
        other => panic!("a folder, got {other:?}"),
    };
    create(&engine, None, ftp("nas", 21, AuthMethod::Anonymous)).await;
    create(&engine, Some(folder), ftp("nas", 21, AuthMethod::Anonymous)).await;
    let ambiguous = engine.locate(&current, "ftp://nas/").await;
    assert_eq!(
        ambiguous.expect_err("two").code(),
        "ftp_connection_ambiguous"
    );
}

#[tokio::test]
async fn the_test_logs_in_with_the_password_from_the_vault() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let (port, seen) = fake_server(PASSWORD).await;
    let nas = create(&engine, None, ftp("nas", port, AuthMethod::Password)).await;

    // Nothing stored: nobody is asked, the test says what is missing.
    assert_eq!(
        code(engine.test_ftp_connection(nas, true).await),
        "password_not_stored"
    );

    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("vault");
    engine
        .dispatch(test_support::store_secret(
            nas,
            SecretKind::Password,
            SecretParam::from(PASSWORD),
        ))
        .await
        .expect("stored");
    let tested = engine
        .test_ftp_connection(nas, true)
        .await
        .expect("logged in");
    assert_eq!(
        tested,
        Outcome::ConnectionTested {
            connection: nas,
            protocol: ConnectionKind::Ftp,
        }
    );
    assert_eq!(*seen.lock().expect("log"), vec![PASSWORD.to_owned()]);
    let answer = serde_json::to_string(&tested).expect("serializes");
    assert!(!answer.contains(PASSWORD), "{answer}");

    // A wrong stored password is the server's refusal.
    engine
        .dispatch(test_support::store_secret(
            nas,
            SecretKind::Password,
            SecretParam::from("not-the-password"),
        ))
        .await
        .expect("stored");
    assert_eq!(
        code(engine.test_ftp_connection(nas, true).await),
        "ftp_login_failed"
    );

    // Locked: the engine asks nobody, it announces the need and refuses - but not for an
    // agent's call over MCP, which must open no unlock dialog.
    let mut events = engine.subscribe();
    engine.dispatch(Command::LockVault).await.expect("locked");
    let needed = |events: &mut tokio::sync::broadcast::Receiver<EngineEvent>| {
        std::iter::from_fn(|| events.try_recv().ok()).any(
            |event| matches!(event, EngineEvent::SecretNeeded { secret } if secret.connection == nas),
        )
    };
    assert_eq!(
        code(engine.test_ftp_connection(nas, false).await),
        "vault_locked"
    );
    assert!(!needed(&mut events), "no SecretNeeded for an MCP call");
    assert_eq!(
        code(engine.test_ftp_connection(nas, true).await),
        "vault_locked"
    );
    assert!(needed(&mut events), "SecretNeeded announced");
}

#[tokio::test]
async fn an_anonymous_login_touches_no_vault() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let (port, seen) = fake_server(ANONYMOUS_PASSWORD).await;
    let public = create(&engine, None, ftp("public", port, AuthMethod::Anonymous)).await;
    engine
        .test_ftp_connection(public, true)
        .await
        .expect("logged in");
    assert_eq!(
        *seen.lock().expect("log"),
        vec![ANONYMOUS_PASSWORD.to_owned()]
    );
}

#[tokio::test]
async fn an_unreachable_server_keeps_its_code_and_no_session() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let port = closed_port().await;
    let gone = create(&engine, None, ftp("gone", port, AuthMethod::Anonymous)).await;
    assert_eq!(
        code(engine.test_ftp_connection(gone, true).await),
        "ftp_connect_failed"
    );
    let listing = engine
        .dispatch(Command::ListDir {
            panel: None,
            path: Some("ftp://gone/pub".into()),
        })
        .await;
    assert_eq!(code(listing), "ftp_connect_failed");
    assert!(engine.inner.remotes.open().is_empty(), "no session kept");
}

#[tokio::test]
async fn only_ftp_connections_are_tested_here() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let ssh = create(&engine, None, ConnectionDraft::new("box", "example.org")).await;
    assert_eq!(
        code(engine.test_ftp_connection(ssh, true).await),
        "ftp_connection_not_found"
    );
    assert_eq!(
        code(engine.test_ftp_connection(ConnectionId(999), true).await),
        "connection_not_found"
    );
}

#[tokio::test]
async fn stat_reads_one_entry_by_its_location() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("notes.txt"), b"twelve bytes").expect("write");
    let engine = engine_in(dir.path()).await;
    let outcome = engine
        .dispatch_mcp(Command::Stat {
            panel: None,
            path: "notes.txt".into(),
        })
        .await
        .expect("stat");
    let Outcome::Stat { path, entry } = outcome else {
        panic!("an entry, got {outcome:?}");
    };
    assert_eq!(path, VfsPath::local(dir.path().join("notes.txt")));
    assert_eq!(entry.kind, EntryKind::File);
    assert_eq!(entry.size, 12);
    assert_eq!(
        code(
            engine
                .dispatch(Command::Stat {
                    panel: None,
                    path: "missing".into(),
                })
                .await
        ),
        "not_found"
    );
}

/// FTPS never falls back to clear text: a server that refuses `AUTH TLS` (explicit) or speaks
/// no TLS at all (implicit) gets neither the user nor the stored password.
#[tokio::test]
async fn ftps_sends_no_password_before_tls() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("vault");
    for kind in [ConnectionKind::Ftps, ConnectionKind::FtpsImplicit] {
        let (port, passwords, commands) = recording_server(PASSWORD).await;
        let draft = ConnectionDraft {
            kind,
            ..ftp(kind.as_str(), port, AuthMethod::Password)
        };
        let id = create(&engine, None, draft).await;
        engine
            .dispatch(test_support::store_secret(
                id,
                SecretKind::Password,
                SecretParam::from(PASSWORD),
            ))
            .await
            .expect("stored");
        let refused = code(engine.test_ftp_connection(id, true).await);
        assert!(
            [
                "ftp_tls_refused",
                "ftp_tls_failed",
                "ftp_connect_failed",
                "ftp_protocol"
            ]
            .contains(&refused),
            "{kind:?}: {refused}"
        );
        assert!(passwords.lock().expect("log").is_empty(), "{kind:?}");
        let sent = commands.lock().expect("log").clone();
        assert!(
            sent.iter()
                .all(|line| !line.contains(PASSWORD)
                    && !line.to_ascii_uppercase().starts_with("USER")),
            "{kind:?}: {sent:?}"
        );
    }
}

/// An agent's list or stat on an FTP location while the vault is locked gets `vault_locked`
/// and nothing else: no `SecretNeeded`, so no unlock dialog opens in the window.
#[tokio::test]
async fn a_locked_vault_opens_no_dialog_for_an_mcp_listing() {
    let dir = tempfile::tempdir().expect("temp dir");
    let engine = engine_in(dir.path()).await;
    let (port, passwords) = fake_server(PASSWORD).await;
    let nas = create(&engine, None, ftp("nas", port, AuthMethod::Password)).await;
    engine
        .dispatch(Command::CreateVault {
            password: SecretParam::from(MASTER),
        })
        .await
        .expect("vault");
    engine
        .dispatch(test_support::store_secret(
            nas,
            SecretKind::Password,
            SecretParam::from(PASSWORD),
        ))
        .await
        .expect("stored");
    engine.dispatch(Command::LockVault).await.expect("locked");
    let mut events = engine.subscribe();
    for command in [
        Command::ListDir {
            panel: None,
            path: Some("ftp://nas/".into()),
        },
        Command::Stat {
            panel: None,
            path: "ftp://nas/pub".into(),
        },
    ] {
        assert_eq!(code(engine.dispatch_mcp(command).await), "vault_locked");
    }
    let announced: Vec<EngineEvent> = std::iter::from_fn(|| events.try_recv().ok()).collect();
    assert!(
        !announced
            .iter()
            .any(|event| matches!(event, EngineEvent::SecretNeeded { .. })),
        "{announced:?}"
    );
    assert!(
        passwords.lock().expect("log").is_empty(),
        "nothing reached the server"
    );
}
