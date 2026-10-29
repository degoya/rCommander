//! `terminal.open_ssh` against the in-process shell server: the PTY, a resize, the end, the start
//! folder quoted, and one TCP connection for the terminal and another channel.
//! `ssh_terminal_login_tests.rs` runs login scripts on it.

use std::time::{Duration, Instant};

use rc_db::Db;
use rc_remote::{
    OfferedKey,
    testkit::{PASSWORD, PtyRequest, ShellServer, host_key_a},
    trust,
};

use super::{cd_command, server_path, shell_quote};
use crate::test_support;
use crate::{
    Command, ConnectionDraft, ConnectionId, ConnectionKind, ConnectionPatch, Engine, EngineError,
    Outcome, SecretKind, SecretParam, SessionState, TermId, TermSize, TerminalInfo,
};

pub(super) struct Harness {
    _root: tempfile::TempDir,
    pub engine: Engine,
    pub server: ShellServer,
    pub id: ConnectionId,
}

/// An engine with an unlocked vault and connection `id` to a shell server whose key is trusted,
/// its password ([`PASSWORD`], also the server's `sudo` password) in the vault.
pub(super) async fn harness() -> Harness {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let engine = test_support::headless(root.path()).await;
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
    let server = ShellServer::start(host_key_a()).await;
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
        .dispatch(test_support::store_secret(
            connection.id,
            SecretKind::Password,
            SecretParam::from(PASSWORD),
        ))
        .await
        .expect("password stored");
    let offered = OfferedKey::from_public_key(host_key_a().public_key()).expect("a key");
    trust::trust(&db, "127.0.0.1", server.port, offered)
        .await
        .expect("trusted");
    Harness {
        _root: root,
        engine,
        server,
        id: connection.id,
    }
}

pub(super) fn open_ssh(id: ConnectionId, cwd: Option<&str>) -> Command {
    Command::OpenSshTerminal {
        connection_id: id,
        panel: None,
        cwd: cwd.map(Into::into),
        size: Some(TermSize {
            rows: 30,
            cols: 100,
        }),
    }
}

pub(super) fn opened(result: Result<Outcome, EngineError>) -> TerminalInfo {
    match result {
        Ok(Outcome::Terminal(info)) => info,
        other => panic!("a terminal: {other:?}"),
    }
}

/// The screen and its scrollback as one text.
pub(super) fn screen(engine: &Engine, terminal: TermId) -> String {
    let session = engine.terminal_session(terminal).expect("the session");
    let screen = session.screen(200);
    [screen.scrollback, screen.lines].concat().join("\n")
}

/// Waits until the screen shows `needle`.
pub(super) async fn shows(engine: &Engine, terminal: TermId, needle: &str) {
    let started = Instant::now();
    loop {
        let text = screen(engine, terminal);
        if text.contains(needle) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "{needle:?} never showed:\n{text}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Waits until `check` holds.
pub(super) async fn eventually(what: &str, check: impl Fn() -> bool) {
    let started = Instant::now();
    while !check() {
        assert!(started.elapsed() < Duration::from_secs(10), "never: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[test]
fn an_sftp_folder_reads_as_the_server_spells_it() {
    use std::path::Path;
    assert_eq!(server_path(Path::new("/home/tester")), "/home/tester");
    assert_eq!(server_path(Path::new("/srv/my site/")), "/srv/my site");
    assert_eq!(server_path(Path::new("/")), "/");
    assert_eq!(server_path(Path::new("")), "/");
    #[cfg(windows)]
    assert_eq!(
        server_path(Path::new("/home/tester\\sub")),
        "/home/tester/sub"
    );
}

#[test]
fn a_folder_is_quoted_for_a_posix_shell() {
    assert_eq!(shell_quote("/srv/www"), "'/srv/www'");
    assert_eq!(shell_quote("/srv/my site"), "'/srv/my site'");
    assert_eq!(
        shell_quote("/srv/it's \"here\""),
        r#"'/srv/it'\''s "here"'"#
    );
    assert_eq!(shell_quote("/srv/$HOME`x`\\"), "'/srv/$HOME`x`\\'");
    assert_eq!(cd_command("/a b").expect("typed"), b"cd '/a b'\r");
    for folder in [
        "/srv/my site",
        "/srv/it's \"here\"",
        "/x/$(rm -rf ~)/\\n",
        "",
    ] {
        assert_eq!(
            rc_remote::testkit::shell_word(&shell_quote(folder)),
            folder,
            "a shell reads the word back as the folder"
        );
    }
}

/// What the review found: a control character ends the quoted line or presses a key.
const HOSTILE: &[&str] = &[
    "a\rrm -rf ~",
    "\x1b[201~",
    "\x03",
    "x\ny",
    "del\x7f",
    "c1\u{9b}2J",
];

#[test]
fn a_folder_with_a_control_character_is_never_typed() {
    for folder in HOSTILE {
        let refused = cd_command(folder).expect_err("refused");
        assert_eq!(refused.code(), "invalid_path", "{folder:?}");
        assert!(
            !refused.to_string().chars().any(char::is_control),
            "the error names it escaped: {refused}"
        );
    }
}

#[tokio::test]
async fn a_hostile_folder_stops_the_open_before_the_server_hears_of_it() {
    let harness = harness().await;
    for folder in HOSTILE {
        let refused = harness
            .engine
            .dispatch(open_ssh(harness.id, Some(folder)))
            .await;
        assert_eq!(
            refused.err().map(|error| error.code()),
            Some("invalid_path"),
            "{folder:?}"
        );
    }
    // The connection tree refuses such a start directory before it is stored (rc-db).
    let stored = harness
        .engine
        .dispatch(Command::UpdateConnection {
            id: harness.id,
            patch: ConnectionPatch {
                start_directory: Some("/srv\rreboot".into()),
                ..ConnectionPatch::default()
            },
        })
        .await;
    assert_eq!(
        stored.err().map(|error| error.code()),
        Some("control_character")
    );
    assert_eq!(harness.server.accepts(), 0, "nothing reached a server");
    assert!(harness.server.lines().is_empty());
}

#[tokio::test]
async fn an_ssh_terminal_opens_in_the_panel_resizes_and_ends_with_its_shell() {
    let harness = harness().await;
    let info = opened(harness.engine.dispatch(open_ssh(harness.id, None)).await);
    assert_eq!(info.shell, "ssh");
    assert_eq!(info.title, "box");
    assert_eq!(info.connection_id, Some(harness.id));
    assert_eq!(info.login_script, None);
    assert!(info.shown, "the panel shows it at once");
    shows(&harness.engine, info.terminal, "tester@test:~$").await;
    assert_eq!(
        harness.server.ptys(),
        vec![PtyRequest {
            term: "xterm-256color".to_owned(),
            cols: 100,
            rows: 30,
        }]
    );

    // As the panel showing it does when it changes size.
    harness
        .engine
        .terminal_session(info.terminal)
        .expect("the session")
        .resize(TermSize {
            rows: 40,
            cols: 132,
        })
        .expect("resized");
    eventually("the server saw the new size", || {
        harness.server.resizes() == vec![(132, 40)]
    })
    .await;

    harness
        .engine
        .dispatch(Command::SendInput {
            terminal: info.terminal,
            text: "exit 7\r".into(),
        })
        .await
        .expect("typed");
    let session = harness
        .engine
        .terminal_session(info.terminal)
        .expect("the session");
    let code = tokio::time::timeout(Duration::from_secs(10), session.wait_exit())
        .await
        .expect("the shell ends");
    assert_eq!(code, Some(7));
    assert_eq!(session.state(), SessionState::Exited { code: Some(7) });
    eventually("the channel let go of the session", || {
        harness
            .engine
            .ssh_session(harness.id)
            .is_some_and(|status| status.channels == 0)
    })
    .await;
}

#[tokio::test]
async fn the_terminal_starts_in_the_folder_however_it_is_spelt() {
    let harness = harness().await;
    let folder = "/srv/it's a \"site\"";
    let info = opened(
        harness
            .engine
            .dispatch(open_ssh(harness.id, Some(folder)))
            .await,
    );
    assert_eq!(info.cwd, std::path::PathBuf::from(folder));
    eventually("the shell went there", || {
        harness.server.cwd().as_deref() == Some(folder)
    })
    .await;
}

#[tokio::test]
async fn without_a_folder_the_connection_s_start_directory_is_taken() {
    let harness = harness().await;
    harness
        .engine
        .dispatch(Command::UpdateConnection {
            id: harness.id,
            patch: ConnectionPatch {
                start_directory: Some("/var/www".into()),
                ..ConnectionPatch::default()
            },
        })
        .await
        .expect("updated");
    opened(harness.engine.dispatch(open_ssh(harness.id, None)).await);
    eventually("the shell went there", || {
        harness.server.cwd().as_deref() == Some("/var/www")
    })
    .await;
}

#[tokio::test]
async fn the_terminal_and_another_channel_share_one_tcp_connection() {
    let harness = harness().await;
    let info = opened(harness.engine.dispatch(open_ssh(harness.id, None)).await);
    shows(&harness.engine, info.terminal, "$").await;
    let other = harness
        .engine
        .ssh_channel(harness.id)
        .await
        .expect("a second channel");
    assert_eq!(
        harness
            .engine
            .ssh_session(harness.id)
            .map(|status| status.channels),
        Some(2)
    );
    assert_eq!(harness.server.accepts(), 1, "one TCP connection");
    drop(other);
}

#[tokio::test]
async fn only_an_ssh_connection_opens_a_terminal() {
    let harness = harness().await;
    let Ok(Outcome::Connection { connection }) = harness
        .engine
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: ConnectionDraft {
                kind: ConnectionKind::Ftp,
                username: Some("web".into()),
                ..ConnectionDraft::new("nas", "127.0.0.1")
            },
        })
        .await
    else {
        panic!("an FTP connection");
    };
    let refused = harness.engine.dispatch(open_ssh(connection.id, None)).await;
    assert_eq!(
        refused.err().map(|error| error.code()),
        Some("connection_not_ssh")
    );
    assert_eq!(harness.server.accepts(), 0, "nothing reached a server");
}
