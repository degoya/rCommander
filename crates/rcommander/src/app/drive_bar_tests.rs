//! The drive bar's logic through `apply`: the entries, a click, the menu's keyboard and the
//! layout setting.

use std::path::Path;

use rc_db::Db;
use rc_engine::{Command, Drive, DriveGroup, DriveKind, DriveState, EngineEvent, PanelId, VfsPath};
use tempfile::TempDir;

use super::{DriveBarLayout, DriveMessage, concerns, load_layout, save_layout};
use crate::app::{Effect, Message, harness::Harness};

fn entry(id: &str, group: DriveGroup, root: Option<&Path>) -> Drive {
    Drive {
        id: id.to_owned(),
        group,
        kind: DriveKind::Fixed,
        label: id.to_owned(),
        root: root.map(VfsPath::local),
        state: DriveState::Ready,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: None,
    }
}

/// A harness on `root`, its drive bar listing `A:` (at `root`), `B:` (elsewhere) and the
/// server `web`.
async fn harness(root: &TempDir) -> Harness {
    let mut harness = Harness::headless(root.path()).await;
    let drives = vec![
        entry("A:", DriveGroup::Drives, Some(root.path())),
        entry("B:", DriveGroup::Drives, Some(Path::new("/elsewhere"))),
        entry("web", DriveGroup::Servers, None),
    ];
    harness
        .send(Message::Drive(DriveMessage::Listed(Ok(drives))))
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
async fn the_listed_entries_name_each_panel_s_drive() {
    let root = tempfile::tempdir().expect("temp dir");
    let harness = harness(&root).await;
    assert_eq!(harness.app.drive_bar.drives.len(), 3);
    let current = harness
        .app
        .current_drive(PanelId::Right)
        .map(|drive| drive.id.as_str());
    assert_eq!(current, Some("A:"));
}

#[tokio::test]
async fn a_click_activates_the_panel_and_changes_its_drive_in_one_job() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    let effects = harness.app.apply(Message::Drive(DriveMessage::Pick(
        PanelId::Right,
        "B:".into(),
    )));
    let (panel, commands) = dispatched(effects);
    assert_eq!(panel, PanelId::Right);
    assert_eq!(
        commands,
        [
            Command::SwitchActivePanel {
                panel: Some(PanelId::Right)
            },
            Command::ChangeDrive {
                panel: Some(PanelId::Right),
                drive: "B:".into()
            },
        ]
    );
}

/// RC-070-03: an FTP server opens like an SSH one; the engine connects on the way.
#[tokio::test]
async fn an_ftp_server_changes_the_drive_like_any_drive() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    let mut ftp = entry("server:5", DriveGroup::Servers, None);
    ftp.kind = DriveKind::Server;
    ftp.root = Some(VfsPath {
        backend: rc_vfs::BackendId::Ftp { connection_id: 5 },
        path: "/".into(),
    });
    harness.app.drive_bar.drives.push(ftp);
    let effects = harness.app.apply(Message::Drive(DriveMessage::Pick(
        PanelId::Left,
        "server:5".into(),
    )));
    let (_, commands) = dispatched(effects);
    assert!(commands.contains(&Command::ChangeDrive {
        panel: Some(PanelId::Left),
        drive: "server:5".into()
    }));
    assert_eq!(harness.app.pane(PanelId::Left).error, None, "no hint");
}

#[tokio::test]
async fn an_ssh_server_changes_the_drive_like_any_drive() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    let mut ssh = entry("server:4", DriveGroup::Servers, None);
    ssh.kind = DriveKind::Server;
    ssh.root = Some(VfsPath {
        backend: rc_vfs::BackendId::Sftp { connection_id: 4 },
        path: "/".into(),
    });
    harness.app.drive_bar.drives.push(ssh);
    let effects = harness.app.apply(Message::Drive(DriveMessage::Pick(
        PanelId::Left,
        "server:4".into(),
    )));
    let (_, commands) = dispatched(effects);
    assert!(commands.contains(&Command::ChangeDrive {
        panel: Some(PanelId::Left),
        drive: "server:4".into()
    }));
}

#[tokio::test]
async fn a_folder_of_servers_opens_the_menu_at_its_first_server() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    for (id, folder) in [("server:7", "Work"), ("server:8", "Work")] {
        let mut server = entry(id, DriveGroup::Servers, None);
        server.folder = vec![folder.to_owned()];
        harness.app.drive_bar.drives.push(server);
    }
    harness
        .send(Message::Drive(DriveMessage::OpenFolder(
            PanelId::Right,
            "Work".into(),
        )))
        .await;
    let menu = harness
        .app
        .drive_bar
        .menu
        .clone()
        .expect("the menu is open");
    assert_eq!(menu.panel, PanelId::Right);
    assert_eq!(
        harness.app.drive_bar.drives[menu.cursor].id, "server:7",
        "on the folder's first server"
    );
}

#[tokio::test]
async fn alt_f1_opens_the_left_menu_which_then_has_the_keyboard() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    harness.press("Alt+F1").await;
    let menu = harness
        .app
        .drive_bar
        .menu
        .clone()
        .expect("the menu is open");
    assert_eq!(menu.panel, PanelId::Left);
    assert_eq!(menu.cursor, 0, "on the panel's drive");

    harness.press("ArrowDown").await;
    assert_eq!(
        harness.app.drive_bar.menu.as_ref().map(|menu| menu.cursor),
        Some(1)
    );
    assert!(
        harness.app.pane(PanelId::Left).on_parent(),
        "the panel's cursor stays"
    );

    harness.press("Alt+F2").await;
    let menu = harness
        .app
        .drive_bar
        .menu
        .clone()
        .expect("the menu is open");
    assert_eq!(menu.panel, PanelId::Right, "Alt+F2 moves to the right menu");
    harness.press("Escape").await;
    assert!(harness.app.drive_bar.menu.is_none());
}

#[tokio::test]
async fn enter_in_the_menu_changes_the_drive_of_the_menu_s_panel() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    harness.app.apply(Message::Engine(EngineEvent::DriveMenu {
        panel: PanelId::Left,
    }));
    harness
        .app
        .apply(Message::Key("ArrowDown".parse().expect("chord")));
    let effects = harness
        .app
        .apply(Message::Key("Enter".parse().expect("chord")));
    let (panel, commands) = dispatched(effects);
    assert_eq!(panel, PanelId::Left);
    assert_eq!(
        commands,
        [Command::ChangeDrive {
            panel: Some(PanelId::Left),
            drive: "B:".into()
        }]
    );
    assert!(harness.app.drive_bar.menu.is_none(), "the menu closed");
}

#[tokio::test]
async fn the_layout_toggles_and_survives_a_restart() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    assert_eq!(harness.app.drive_bar.layout, DriveBarLayout::PerPanel);
    harness
        .send(Message::Drive(DriveMessage::ToggleLayout))
        .await;
    assert_eq!(harness.app.drive_bar.layout, DriveBarLayout::Shared);

    let db = Db::open_in_memory().await.expect("db");
    assert_eq!(
        load_layout(&db).await,
        DriveBarLayout::PerPanel,
        "the default"
    );
    save_layout(&db, DriveBarLayout::Shared)
        .await
        .expect("stores");
    assert_eq!(load_layout(&db).await, DriveBarLayout::Shared);
}

#[test]
fn the_drive_bar_takes_its_own_events_only() {
    assert!(concerns(&EngineEvent::Drives));
    assert!(concerns(&EngineEvent::DriveMenu {
        panel: PanelId::Right
    }));
    assert!(!concerns(&EngineEvent::WslDistros));
    assert!(!concerns(&EngineEvent::ActivePanel {
        panel: PanelId::Left
    }));
}

/// The owner's Windows test (2026-10-18): after the Total Commander import the menu is longer
/// than its card, and the cursor went below its edge. Opening the menu and every move of its
/// cursor scroll it into view.
#[tokio::test]
async fn every_move_of_the_menu_s_cursor_scrolls_it_into_view() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    let scrolls = |effects: &[Effect]| {
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::Task(_)))
            .count()
    };
    let opened = harness.app.apply(Message::Engine(EngineEvent::DriveMenu {
        panel: PanelId::Left,
    }));
    assert_eq!(scrolls(&opened), 1, "the menu opens at the panel's drive");
    for keys in ["ArrowDown", "ArrowUp", "PageDown", "PageUp", "End", "Home"] {
        let moved = harness
            .app
            .apply(Message::Key(keys.parse().expect("chord")));
        assert_eq!(scrolls(&moved), 1, "{keys}");
    }
    let folder = harness.app.apply(Message::Drive(DriveMessage::OpenFolder(
        PanelId::Right,
        "web".into(),
    )));
    assert_eq!(
        scrolls(&folder),
        1,
        "a folder's button opens at its servers"
    );
    let closed = harness
        .app
        .apply(Message::Key("Escape".parse().expect("chord")));
    assert_eq!(scrolls(&closed), 0);
}
