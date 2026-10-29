//! What the manager opens on a connection: Enter on an SSH server, Open in panel, Terminal,
//! Run command, and the trusted certificate of an FTPS server.

use rc_db::TrustedCertificate;
use rc_engine::{
    AuthMethod, Command, ConnectionDraft, ConnectionId, ConnectionKind, Outcome, TreeItem,
};
use tempfile::TempDir;

use super::super::ConnMsg;
use crate::app::{Effect, Message, harness::Harness};

async fn harness() -> (TempDir, Harness) {
    Harness::temp_with_db().await
}

async fn create(harness: &Harness, connection: ConnectionDraft) -> ConnectionId {
    let command = Command::CreateConnection {
        folder_id: None,
        connection,
    };
    match harness
        .app
        .engine()
        .dispatch(command)
        .await
        .expect("created")
    {
        Outcome::Connection { connection } => connection.id,
        other => panic!("a connection, got {other:?}"),
    }
}

/// An SSH server nobody listens on: a connect fails at once, without a network.
fn closed_ssh(name: &str) -> ConnectionDraft {
    ConnectionDraft {
        port: Some(1),
        ..ConnectionDraft::new(name, "127.0.0.1")
    }
}

fn ftps(name: &str, host: &str) -> ConnectionDraft {
    ConnectionDraft {
        kind: ConnectionKind::FtpsImplicit,
        username: Some("backup".into()),
        auth_method: AuthMethod::Password,
        ..ConnectionDraft::new(name, host)
    }
}

fn trusted(id: i64, host: &str, port: u16) -> TrustedCertificate {
    TrustedCertificate {
        id,
        host: host.into(),
        port,
        fingerprint_sha256: "AB:CD".into(),
        subject: "CN=nas".into(),
        issuer: "CN=nas".into(),
        not_after: 0,
        added_at: "2026-10-18T00:00:00Z".into(),
        last_seen_at: None,
    }
}

#[tokio::test]
async fn enter_opens_an_ssh_and_an_ftp_server_alike() {
    let (_root, mut harness) = harness().await;
    let ssh = create(&harness, closed_ssh("web")).await;
    let ftp = create(&harness, ftps("nas", "nas.local")).await;
    harness.press("Ctrl+F").await;
    let manager = harness.app.connections.as_mut().expect("open");

    for id in [ssh, ftp] {
        let effects = manager.open_connection(id);
        assert!(matches!(effects.as_slice(), [Effect::Task(_)]));
        assert_eq!(manager.note, None);
    }
}

#[tokio::test]
async fn run_command_opens_its_dialog_over_the_manager() {
    let (_root, mut harness) = harness().await;
    let id = create(&harness, closed_ssh("web")).await;
    harness.press("Ctrl+F").await;
    harness
        .send(Message::Connections(ConnMsg::RunCommand(id)))
        .await;
    let dialog = harness.app.ssh_exec.as_ref().expect("the dialog is open");
    assert_eq!(dialog.connection, id);
    assert_eq!(dialog.name, "web");
    assert!(harness.app.connections.is_some(), "the manager stays below");
}

#[tokio::test]
async fn open_in_panel_and_terminal_close_the_manager() {
    let (_root, mut harness) = harness().await;
    let id = create(&harness, closed_ssh("web")).await;
    for message in [ConnMsg::Files(id), ConnMsg::Terminal(id)] {
        harness.press("Ctrl+F").await;
        assert!(harness.app.connections.is_some());
        harness.send(Message::Connections(message)).await;
        assert!(harness.app.connections.is_none(), "the panel is in sight");
        let active = harness.app.active;
        assert!(
            harness.app.pane(active).error.is_some(),
            "nobody listens: the panel says so"
        );
    }
}

#[tokio::test]
async fn an_ftps_connection_shows_the_certificates_of_its_server_only() {
    let (_root, mut harness) = harness().await;
    let nas = create(&harness, ftps("nas", "NAS.local")).await;
    let ssh = create(&harness, closed_ssh("web")).await;
    harness.press("Ctrl+F").await;
    let manager = harness.app.connections.as_mut().expect("open");
    manager.certificates = vec![
        trusted(1, "nas.local", 990),
        trusted(2, "nas.local", 21),
        trusted(3, "other.local", 990),
    ];
    let shown: Vec<i64> = manager.certificates_of(nas).iter().map(|c| c.id).collect();
    assert_eq!(shown, [1]);
    assert!(manager.certificates_of(ssh).is_empty());
}

#[tokio::test]
async fn shift_and_ctrl_enter_act_on_an_ssh_server_only() {
    let (_root, mut harness) = harness().await;
    let ssh = create(&harness, closed_ssh("web")).await;
    let ftp = create(&harness, ftps("nas", "nas.local")).await;
    harness.press("Ctrl+F").await;
    let manager = harness.app.connections.as_mut().expect("open");

    manager.tree.cursor = Some(TreeItem::Connection(ftp));
    assert!(manager.terminal_key().is_empty(), "no terminal on FTP");
    assert!(manager.connect_key().is_empty(), "no session on FTP");

    manager.tree.cursor = Some(TreeItem::Connection(ssh));
    assert!(matches!(
        manager.terminal_key().as_slice(),
        [Effect::Task(_)]
    ));
    harness.press("Ctrl+Enter").await;
    let manager = harness
        .app
        .connections
        .as_ref()
        .expect("Connect keeps it open");
    assert!(
        manager.note.as_ref().is_some_and(|(_, error)| *error),
        "nobody listens: the note says so, {:?}",
        manager.note
    );
}
