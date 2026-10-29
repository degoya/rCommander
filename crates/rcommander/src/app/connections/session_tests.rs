//! Ctrl+Shift+F, Test, and the login-script editor over the manager, through `apply` and a real
//! engine on an in-memory database.

use rc_engine::{
    AuthMethod, Chord, Command, ConnectionDraft, ConnectionId, ConnectionKind, Outcome, PanelView,
    VfsPath,
};
use rc_vfs::BackendId;
use tempfile::TempDir;

use super::super::ConnMsg;
use crate::{
    app::{Effect, Message, harness::Harness},
    i18n::error_text,
};

async fn harness() -> (TempDir, Harness) {
    Harness::temp_with_db().await
}

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

#[tokio::test]
async fn ctrl_shift_f_closes_the_connection_of_the_active_panel() {
    let (_root, mut harness) = harness().await;
    let keys = chord("Ctrl+Shift+F");
    // A local folder has no connection: the key is taken and does nothing.
    let local = harness.app.disconnect_key(&keys, PanelView::Files);
    assert!(local.is_some_and(|effects| effects.is_empty()));
    let active = harness.app.active;
    harness.app.panes[active.index()].state.path = VfsPath {
        backend: BackendId::Ftp { connection_id: 7 },
        path: "/pub".into(),
    };
    let effects = harness
        .app
        .disconnect_key(&keys, PanelView::Files)
        .expect("taken");
    assert!(
        matches!(
            effects.as_slice(),
            [Effect::Dispatch(panel, commands)]
                if *panel == active
                    && commands.as_slice()
                        == [Command::DisconnectSession { id: ConnectionId(7) }]
        ),
        "one disconnect of connection 7"
    );
    // A terminal keeps the chord, and other chords are the keymap's.
    assert!(
        harness
            .app
            .disconnect_key(&keys, PanelView::Terminal)
            .is_none()
    );
    assert!(
        harness
            .app
            .disconnect_key(&chord("Ctrl+F"), PanelView::Files)
            .is_none()
    );
}

#[tokio::test]
async fn test_reports_how_the_stored_connection_went() {
    let (_root, mut harness) = harness().await;
    // A port nothing listens on.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("a free port")
        .port();
    let draft = ConnectionDraft {
        kind: ConnectionKind::Ftp,
        port: Some(i64::from(port)),
        auth_method: AuthMethod::Anonymous,
        ..ConnectionDraft::new("gone", "127.0.0.1")
    };
    let created = harness
        .app
        .engine()
        .dispatch(Command::CreateConnection {
            folder_id: None,
            connection: draft,
        })
        .await
        .expect("created");
    let Outcome::Connection { connection } = created else {
        panic!("a connection, got {created:?}");
    };
    harness.press("Ctrl+F").await;
    harness
        .send(Message::Connections(ConnMsg::Test(connection.id)))
        .await;
    let manager = harness.app.connections.as_ref().expect("open");
    assert_eq!(
        manager.note,
        Some((error_text("ftp_connect_failed"), true)),
        "the failure stands under the form"
    );
}

#[tokio::test]
async fn the_login_script_editor_opens_over_the_manager_and_escape_closes_it_first() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;
    assert!(
        harness.app.connections.is_some(),
        "Ctrl+F opens the manager"
    );
    harness
        .send(Message::Connections(ConnMsg::LoginScripts))
        .await;
    assert!(harness.app.login_scripts.is_some(), "the editor is open");
    harness.press("Escape").await;
    assert!(
        harness.app.login_scripts.is_none(),
        "Escape closes the editor"
    );
    assert!(harness.app.connections.is_some(), "and leaves the manager");
    harness
        .send(Message::Connections(ConnMsg::LoginScripts))
        .await;
    harness.send(Message::CloseLoginScripts).await;
    assert!(
        harness.app.login_scripts.is_none(),
        "Close closes it as well"
    );
}

/// Through the window's keys: Ctrl+Shift+F on a panel that shows an FTP connection reaches the
/// engine as `connections.disconnect` - for a connection that is gone, its error stands in the
/// panel's status line.
#[tokio::test]
async fn ctrl_shift_f_reaches_the_engine_from_the_file_list() {
    let (_root, mut harness) = harness().await;
    let active = harness.app.active;
    harness.app.panes[active.index()].state.path = VfsPath {
        backend: BackendId::Ftp { connection_id: 99 },
        path: "/".into(),
    };
    harness.press("Ctrl+Shift+F").await;
    assert_eq!(
        harness.app.pane(active).error,
        Some(error_text("connection_not_found"))
    );
}
