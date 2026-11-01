//! The drive bar's logic through `apply`: the entries, a click, the menu's keyboard and the
//! layout setting.

use std::path::Path;

use rc_db::Db;
use rc_engine::{Command, Drive, DriveGroup, DriveKind, DriveState, EngineEvent, PanelId, VfsPath};
use tempfile::TempDir;

use super::{
    DriveBarLayout, DriveMessage, concerns, load_layout,
    menu::{MenuLine, Scope, lines},
    place::Anchor,
    save_layout,
};
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
        folder_buttons: Vec::new(),
        remote: None,
    }
}

/// The id of the entry at `root`: on Windows the letter of the drive `root` is on, which is what
/// names a panel's drive there; `A:` elsewhere.
fn here(root: &TempDir) -> String {
    rc_vfs::drive_letter(&root.path().to_string_lossy()).map_or_else(
        || "A:".to_owned(),
        |letter| format!("{}:", letter.to_ascii_uppercase()),
    )
}

/// A harness on `root`, its drive bar listing [`here`] (at `root`), `B:` (elsewhere) and the
/// server `web`.
async fn harness(root: &TempDir) -> Harness {
    let mut harness = Harness::headless(root.path()).await;
    let drives = vec![
        entry(&here(root), DriveGroup::Drives, Some(root.path())),
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
        .map(|drive| drive.id.clone());
    assert_eq!(current, Some(here(&root)));
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

/// A folder's button opens a menu of its servers only, under the button once it is measured.
#[tokio::test]
async fn a_folder_of_servers_opens_a_menu_of_its_servers() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    for (id, folder) in [("server:7", "Work"), ("server:8", "Work")] {
        let mut server = entry(id, DriveGroup::Servers, None);
        server.folder = vec![folder.to_owned()];
        server.folder_buttons = vec![1];
        harness.app.drive_bar.drives.push(server);
    }
    harness
        .send(Message::Drive(DriveMessage::OpenFolder(
            PanelId::Right,
            vec!["Work".into()],
        )))
        .await;
    let menu = harness
        .app
        .drive_bar
        .menu
        .clone()
        .expect("the menu is open");
    assert_eq!(menu.panel, PanelId::Right);
    assert_eq!(menu.scope, Scope::Folder(vec!["Work".into()]));
    assert!(
        menu.actions.is_empty(),
        "the network actions are the full menu's"
    );
    let drives = &harness.app.drive_bar.drives;
    let listed: Vec<&str> = lines(drives, &menu)
        .into_iter()
        .filter_map(|line| match line {
            MenuLine::Entry { drive, .. } => Some(drives[drive].id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(listed, ["server:7", "server:8"]);
    assert_eq!(menu.cursor, 0, "on the folder's first server");
    assert_eq!(harness.app.drive_bar.anchor, Anchor::Measuring);
}

/// A click opens the menu under the button once it is measured; the keyboard opens it where it
/// always stood, and a measurement that arrives after it is left alone. A button no longer drawn
/// leaves the menu where the keyboard's stands.
#[tokio::test]
async fn a_click_anchors_the_menu_under_its_button_the_keyboard_does_not() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    let button = iced::Rectangle {
        x: 400.0,
        y: 60.0,
        width: 30.0,
        height: 20.0,
    };
    harness
        .send(Message::Drive(DriveMessage::OpenMenu(PanelId::Right)))
        .await;
    assert_eq!(harness.app.drive_bar.anchor, Anchor::Measuring);
    let menu = harness
        .app
        .drive_bar
        .menu
        .as_ref()
        .expect("the menu is open");
    assert_eq!(
        menu.scope,
        Scope::All,
        "the drive line's menu lists everything"
    );
    harness
        .send(Message::Drive(DriveMessage::Anchored(Some(button))))
        .await;
    assert_eq!(harness.app.drive_bar.anchor, Anchor::Under(button));

    harness.press("Alt+F1").await;
    assert_eq!(
        harness.app.drive_bar.anchor,
        Anchor::Fixed,
        "Alt+F1 switched"
    );
    harness
        .send(Message::Drive(DriveMessage::Anchored(Some(button))))
        .await;
    assert_eq!(harness.app.drive_bar.anchor, Anchor::Fixed, "a late answer");

    harness
        .send(Message::Drive(DriveMessage::OpenOverflow(PanelId::Left, 1)))
        .await;
    let menu = harness
        .app
        .drive_bar
        .menu
        .as_ref()
        .expect("the menu is open");
    assert_eq!(menu.scope, Scope::Overflow(1));
    harness
        .send(Message::Drive(DriveMessage::Anchored(None)))
        .await;
    assert_eq!(harness.app.drive_bar.anchor, Anchor::Fixed);
}

/// A drive's context menu from a bar button stands under that button; from the drive menu it
/// takes the menu's place.
#[tokio::test]
async fn a_drive_s_context_menu_takes_its_button_or_the_menu_s_place() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = harness(&root).await;
    harness.app.drive_bar.network = true;
    let effects = harness.app.apply(Message::Drive(DriveMessage::Context(
        PanelId::Left,
        "B:".into(),
    )));
    assert!(harness.app.drive_bar.context.is_some());
    assert_eq!(effects.len(), 1, "the button is measured");
    assert_eq!(harness.app.drive_bar.anchor, Anchor::Measuring);

    let button = iced::Rectangle {
        x: 10.0,
        y: 40.0,
        width: 30.0,
        height: 20.0,
    };
    harness
        .send(Message::Drive(DriveMessage::OpenMenu(PanelId::Left)))
        .await;
    harness
        .send(Message::Drive(DriveMessage::Anchored(Some(button))))
        .await;
    let effects = harness
        .app
        .apply(Message::Key("Shift+F10".parse().expect("chord")));
    assert!(effects.is_empty(), "nothing measured");
    assert!(harness.app.drive_bar.menu.is_none());
    assert!(harness.app.drive_bar.context.is_some());
    assert_eq!(harness.app.drive_bar.anchor, Anchor::Under(button));
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
        vec!["web".into()],
    )));
    assert_eq!(
        scrolls(&folder),
        2,
        "a folder's button opens at its servers, measured for its place"
    );
    let closed = harness
        .app
        .apply(Message::Key("Escape".parse().expect("chord")));
    assert_eq!(scrolls(&closed), 0);
}
