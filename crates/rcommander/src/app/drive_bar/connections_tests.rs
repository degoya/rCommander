//! The open connections in the drive bar through `apply` (RC-110-38): a button appears once its
//! session is listed, a click changes to it, its context menu disconnects it, and it goes with
//! its session or with the setting off.

use rc_engine::{
    Command, ConnectionId, ConnectionKind, Drive, DriveGroup, DriveKind, DriveState, EngineEvent,
    OpenConnection, PanelId, VfsPath,
};

use crate::app::{
    Effect, Message,
    drive_bar::{
        DriveMessage, concerns,
        line::LineSettings,
        menu::{MenuLine, Scope, lines},
        overflow::{BarItem, bar_items},
    },
    harness::Harness,
};

/// The server `server:<id>` inside the folder `Work`, which is no button of the bar.
fn server(id: i64) -> Drive {
    Drive {
        id: format!("server:{id}"),
        group: DriveGroup::Servers,
        kind: DriveKind::Server,
        label: format!("box{id}"),
        root: Some(VfsPath {
            backend: rc_vfs::BackendId::Ftp { connection_id: id },
            path: "/".into(),
        }),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: vec!["Work".to_owned()],
        favourite: false,
        folder_buttons: Vec::new(),
        remote: None,
    }
}

fn open(id: i64) -> OpenConnection {
    OpenConnection {
        drive: format!("server:{id}"),
        connection_id: ConnectionId(id),
        name: format!("box{id}"),
        kind: ConnectionKind::Ftp,
        last_dir: None,
    }
}

/// Whether the bar shows the entry `id` as a button of its own.
fn has_button(harness: &Harness, id: &str) -> bool {
    let drives = &harness.app.drive_bar.drives;
    bar_items(drives)
        .into_iter()
        .any(|item| matches!(item, BarItem::Drive(index) if drives[index].id == id))
}

/// A harness whose drive bar lists the servers 4 and 5, neither a button; 4 has a session open.
async fn harness(root: &tempfile::TempDir) -> Harness {
    let mut harness = Harness::headless(root.path()).await;
    harness
        .send(Message::Drive(DriveMessage::Listed(Ok(vec![
            server(4),
            server(5),
        ]))))
        .await;
    assert!(
        !has_button(&harness, "server:4"),
        "no button before its session"
    );
    harness
        .send(Message::Drive(DriveMessage::Connections(Ok(vec![open(4)]))))
        .await;
    harness
}

/// The commands of the one dispatch among `effects`.
fn dispatched(effects: Vec<Effect>) -> (PanelId, Vec<Command>) {
    let mut found = effects.into_iter().filter_map(|effect| match effect {
        Effect::Dispatch(panel, commands) => Some((panel, commands)),
        _ => None,
    });
    let first = found.next().expect("a dispatch");
    assert!(found.next().is_none(), "one dispatch");
    first
}

#[tokio::test]
async fn an_open_connection_gets_a_button_which_changes_to_it() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    assert!(has_button(&harness, "server:4"), "the open one");
    assert!(!has_button(&harness, "server:5"), "not the closed one");
    assert!(harness.app.drive_bar.is_open("server:4"));

    // A new list of the drives keeps the button: the two lists are merged anew.
    harness
        .send(Message::Drive(DriveMessage::Listed(Ok(vec![
            server(4),
            server(5),
        ]))))
        .await;
    assert!(has_button(&harness, "server:4"));

    // The engine returns to the folder last shown there (`panel.change_drive`).
    let effects = harness.app.apply(Message::Drive(DriveMessage::Pick(
        PanelId::Left,
        "server:4".into(),
    )));
    let (_, commands) = dispatched(effects);
    assert!(commands.contains(&Command::ChangeDrive {
        panel: Some(PanelId::Left),
        drive: "server:4".into()
    }));

    // Its session ends: the button goes.
    harness
        .send(Message::Drive(DriveMessage::Connections(Ok(Vec::new()))))
        .await;
    assert!(!has_button(&harness, "server:4"));
}

#[tokio::test]
async fn the_context_menu_of_an_open_connection_disconnects_it() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    harness.app.apply(Message::Drive(DriveMessage::Context(
        PanelId::Right,
        "server:4".into(),
    )));
    let menu = harness
        .app
        .drive_bar
        .menu
        .clone()
        .expect("its context menu");
    assert_eq!(menu.scope, Scope::Connection("server:4".into()));
    assert_eq!(
        lines(&harness.app.drive_bar.drives, &menu),
        [
            MenuLine::Entry {
                stop: 0,
                drive: 0,
                indent: false
            },
            MenuLine::Disconnect { stop: 1, drive: 0 },
        ]
    );

    harness
        .app
        .apply(Message::Key("ArrowDown".parse().expect("chord")));
    let effects = harness
        .app
        .apply(Message::Key("Enter".parse().expect("chord")));
    let (panel, commands) = dispatched(effects);
    assert_eq!(panel, PanelId::Right);
    assert_eq!(
        commands,
        [Command::DisconnectSession {
            id: ConnectionId(4)
        }]
    );
    assert!(harness.app.drive_bar.menu.is_none(), "the menu closed");

    // A click on the row does the same.
    let effects = harness.app.apply(Message::Drive(DriveMessage::Disconnect(
        PanelId::Left,
        "server:4".into(),
    )));
    assert_eq!(
        dispatched(effects),
        (
            PanelId::Left,
            vec![Command::DisconnectSession {
                id: ConnectionId(4)
            }]
        )
    );

    // A server without a session has no context menu.
    harness.app.apply(Message::Drive(DriveMessage::Context(
        PanelId::Right,
        "server:5".into(),
    )));
    assert!(harness.app.drive_bar.menu.is_none());
    assert!(harness.app.drive_bar.context.is_none());
}

/// The context menu closes with the session it was opened for.
#[tokio::test]
async fn a_connection_s_context_menu_closes_with_its_session() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    harness.app.apply(Message::Drive(DriveMessage::Context(
        PanelId::Left,
        "server:4".into(),
    )));
    assert!(harness.app.drive_bar.menu.is_some());
    harness
        .send(Message::Drive(DriveMessage::Connections(Ok(Vec::new()))))
        .await;
    assert!(harness.app.drive_bar.menu.is_none());
}

/// Shift+F10 on an open connection's row of the drive menu opens its context menu in the menu's
/// place; Shift+F10 there closes it, as a drive's context menu closes.
#[tokio::test]
async fn shift_f10_in_the_drive_menu_opens_a_connection_s_context_menu() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    harness.app.apply(Message::Engine(EngineEvent::DriveMenu {
        panel: PanelId::Left,
    }));
    harness
        .app
        .apply(Message::Key("Shift+F10".parse().expect("chord")));
    let scope = harness
        .app
        .drive_bar
        .menu
        .as_ref()
        .map(|menu| menu.scope.clone());
    assert_eq!(scope, Some(Scope::Connection("server:4".into())));
    harness
        .app
        .apply(Message::Key("Shift+F10".parse().expect("chord")));
    assert!(harness.app.drive_bar.menu.is_none());
}

/// `drive_bar.show_connections` off: no button, no mark, no context menu.
#[tokio::test]
async fn the_setting_off_shows_no_open_connection() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    let off = LineSettings {
        connections: false,
        ..LineSettings::default()
    };
    harness.send(Message::Drive(DriveMessage::Line(off))).await;
    assert!(!has_button(&harness, "server:4"));
    assert!(!harness.app.drive_bar.is_open("server:4"));
    harness.app.apply(Message::Drive(DriveMessage::Context(
        PanelId::Left,
        "server:4".into(),
    )));
    assert!(harness.app.drive_bar.menu.is_none());

    harness
        .send(Message::Drive(DriveMessage::Line(LineSettings::default())))
        .await;
    assert!(has_button(&harness, "server:4"), "on again");
}

#[test]
fn a_session_s_start_and_end_reach_the_drive_bar() {
    assert!(concerns(&EngineEvent::Session {
        connection: ConnectionId(4),
        change: rc_engine::SessionChange::Connected,
    }));
}
