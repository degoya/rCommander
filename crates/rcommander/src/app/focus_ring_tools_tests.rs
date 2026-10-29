//! The focus ring (RC-080-16) of the tool dialogs: the diff window, synchronize with its
//! question, the checksum dialogs and their result list, the queue window, the database dialog,
//! the hotlist editor, the network drive dialogs and the question before a large fetch. Each
//! walks round every stop in reading order, Enter and Space press each button and switch, and no
//! button is left out but the X of the head.

use std::sync::Arc;

use iced::{Element, keyboard::key::Named, widget::Space};
use rc_engine::{
    Command, Drive, DriveGroup, DriveKind, DriveState, HotlistEntry, JobOptions, PanelId, VfsPath,
};

use super::probe::{
    Screen, beside_a_file, first_of, harness, kinds, press_at, press_every_stop, press_on,
    two_trees, walk_round,
};
use crate::{
    app::{
        Message,
        checksum::{self, ChecksumMsg, ChecksumWindow},
        compare::{self, CompareMsg, DiffOption, Flag as SyncFlag},
        database_dialog::{self, DatabaseMsg},
        drag::DragMsg,
        drive_bar::{self, DriveMessage, actions::DriveAction, network::NetworkMsg},
        queue_window::{ListMsg, MenuMsg, QueueMsg},
        tabs::{self, TabsMessage, editor::HotlistEditor},
    },
    theme::{self, focus::Kind},
};

fn nothing<'a>() -> Element<'a, Message> {
    Space::new().into()
}

#[tokio::test]
async fn the_diff_window_walks_its_switches_rows_and_buttons() {
    let (_root, mut harness) = two_trees().await;
    harness.press("Ctrl+Shift+F3").await;
    let window = harness.app.compare.diff.as_ref().expect("the window");
    let comparison = compare::load(
        harness.app.engine().clone(),
        window.left.clone(),
        window.right.clone(),
        window.options,
        window.cancel.clone(),
    );
    let generation = window.generation;
    let loaded = comparison.await;
    harness
        .send(Message::Compare(CompareMsg::DiffLoaded(generation, loaded)))
        .await;
    let layer = harness.app.compare_layer(nothing(), theme::tokens(false));
    let mut screen = Screen::new(layer).await;
    let stops = walk_round(&mut screen, "diff window");
    assert_eq!(
        kinds(&stops)[..4],
        [
            Some(Kind::Toggle),
            Some(Kind::Toggle),
            Some(Kind::Toggle),
            Some(Kind::Choice)
        ],
        "the three switches, then the rows"
    );
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "diff window");
    let case = press_on(&mut screen, "Ignore case", Named::Space);
    assert!(matches!(
        case[..],
        [Message::Compare(CompareMsg::DiffOption(
            DiffOption::Case,
            true
        ))]
    ));
    let next = press_on(&mut screen, "Next difference", Named::Enter);
    assert!(matches!(next[..], [Message::Compare(CompareMsg::DiffNext)]));
}

/// The dialog's form, filters, rows and buttons; then the question before the jobs over it,
/// with a ring of its own.
#[tokio::test]
async fn synchronize_and_its_question_walk_their_controls() {
    let (_root, mut harness) = two_trees().await;
    let tokens = theme::tokens(false);
    harness.press("Alt+Shift+F2").await;
    {
        let mut screen = Screen::new(harness.app.compare_layer(nothing(), tokens)).await;
        let stops = walk_round(&mut screen, "synchronize");
        assert_eq!(stops[0].kind, Some(Kind::Choice), "the mode first");
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "synchronize");
        let subdirs = press_on(&mut screen, "Subdirectories", Named::Space);
        assert!(matches!(
            subdirs[..],
            [Message::Compare(CompareMsg::SyncFlag(SyncFlag::Subdirs, _))]
        ));
        let close = press_on(&mut screen, "Close", Named::Enter);
        assert!(matches!(
            close[..],
            [Message::Compare(CompareMsg::SyncClose)]
        ));
    }
    // Compare, the plan as the task hands it over, Synchronize.
    harness.press("Enter").await;
    let dialog = harness.app.compare.sync.as_ref().expect("the dialog");
    let options = dialog.form.options().expect("options");
    let plan = harness
        .app
        .engine()
        .plan_sync(&dialog.left, &dialog.right, &options, &dialog.cancel)
        .await
        .map(Arc::new)
        .map_err(|error| error.code());
    let generation = dialog.generation;
    harness
        .send(Message::Compare(CompareMsg::SyncPlanned(generation, plan)))
        .await;
    harness.send(Message::Compare(CompareMsg::SyncAsk)).await;
    let dialog = harness.app.compare.sync.as_ref().expect("the dialog");
    assert!(dialog.confirm.is_some(), "the question");
    let mut screen = Screen::new(harness.app.compare_layer(nothing(), tokens)).await;
    let stops = walk_round(&mut screen, "synchronize question");
    assert_eq!(
        kinds(&stops),
        [Some(Kind::Toggle), Some(Kind::Button), Some(Kind::Button)],
        "ask before replacing, Synchronize, Cancel"
    );
    assert_eq!(screen.loose_buttons(), 2, "the X of each head");
    let ask = press_on(&mut screen, "Ask before replacing", Named::Space);
    assert!(matches!(
        ask[..],
        [Message::Compare(CompareMsg::SyncFlag(
            SyncFlag::AskOverwrite,
            _
        ))]
    ));
    let cancel = press_on(&mut screen, "Cancel", Named::Enter);
    assert!(matches!(
        cancel[..],
        [Message::Compare(CompareMsg::SyncCancelConfirm)]
    ));
}

/// Create, split and combine, then the result list.
#[tokio::test]
async fn the_checksum_dialogs_walk_their_lists_fields_and_buttons() {
    let (_root, mut harness) = beside_a_file().await;
    let tokens = theme::tokens(false);
    harness.press("Ctrl+Shift+F9").await;
    {
        let window = harness.app.checksum.as_ref().expect("create");
        let mut screen = Screen::new(checksum::overlay(window, tokens)).await;
        let stops = walk_round(&mut screen, "create checksums");
        assert_eq!(
            kinds(&stops)[..3],
            [Some(Kind::Choice), Some(Kind::Toggle), None],
            "the checksum, one file each, the name"
        );
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "create checksums");
        let algorithm = press_at(&mut screen, 0, Named::ArrowDown);
        assert!(matches!(
            algorithm[..],
            [Message::Checksum(ChecksumMsg::Algorithm(_))]
        ));
        let each = press_on(&mut screen, "One checksum file for each", Named::Space);
        assert!(matches!(
            each[..],
            [Message::Checksum(ChecksumMsg::PerFile(true))]
        ));
    }
    harness.send(Message::Checksum(ChecksumMsg::Close)).await;
    harness.press("Ctrl+Shift+F11").await;
    {
        let window = harness.app.checksum.as_ref().expect("split");
        let mut screen = Screen::new(checksum::overlay(window, tokens)).await;
        let stops = walk_round(&mut screen, "split");
        assert_eq!(stops[0].kind, None, "the folder first");
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "split");
        let preset = press_at(
            &mut screen,
            first_of(&stops, Kind::Choice),
            Named::ArrowDown,
        );
        assert!(matches!(
            preset[..],
            [Message::Checksum(ChecksumMsg::Preset(_))]
        ));
        let cancel = press_on(&mut screen, "Cancel", Named::Enter);
        assert!(matches!(
            cancel[..],
            [Message::Checksum(ChecksumMsg::Close)]
        ));
    }
    harness.send(Message::Checksum(ChecksumMsg::Close)).await;
    harness.press("Ctrl+Shift+F12").await;
    {
        let window = harness.app.checksum.as_ref().expect("combine");
        let mut screen = Screen::new(checksum::overlay(window, tokens)).await;
        let stops = walk_round(&mut screen, "combine");
        assert_eq!(stops[0].kind, None, "the folder first");
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "combine");
        let combine = press_on(&mut screen, "Combine", Named::Enter);
        assert!(matches!(
            combine[..],
            [Message::Checksum(ChecksumMsg::Confirm)]
        ));
    }
    harness.send(Message::Checksum(ChecksumMsg::Close)).await;
    harness.app.checksum = Some(ChecksumWindow::Results(Vec::new()));
    let window = harness.app.checksum.as_ref().expect("the results");
    let mut screen = Screen::new(checksum::overlay(window, tokens)).await;
    let stops = walk_round(&mut screen, "checksum results");
    assert_eq!(kinds(&stops), [Some(Kind::Button)], "Close");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    let close = press_on(&mut screen, "Close", Named::Enter);
    assert!(matches!(close[..], [Message::Checksum(ChecksumMsg::Close)]));
}

#[tokio::test]
async fn the_queue_window_walks_its_lists_and_buttons() {
    let (_root, mut harness) = beside_a_file().await;
    harness.press("Ctrl+Shift+Q").await;
    assert!(harness.app.queue_window.is_some(), "the window");
    let layer = harness.app.queue_layer(nothing(), theme::tokens(false));
    let mut screen = Screen::new(layer).await;
    let stops = walk_round(&mut screen, "queue window");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "queue window");
    let pause = press_on(&mut screen, "Pause all", Named::Enter);
    assert!(matches!(pause[..], [Message::Queue(QueueMsg::PauseAll)]));
    let close = press_on(&mut screen, "Close", Named::Enter);
    assert!(matches!(close[..], [Message::Queue(QueueMsg::Close)]));
}

/// RC-010-15: over a selection, with the context menu open and then with the question before a
/// list is cleared, every button is a stop of the ring; the menu's entries are no loose buttons.
#[tokio::test]
async fn the_queue_window_walks_its_menu_and_its_question() {
    let (_root, mut harness) = beside_a_file().await;
    // A stopped list holds the copy: it stays waiting.
    let held = harness
        .app
        .engine()
        .ops()
        .create_list("held")
        .expect("made");
    let copy = Command::OpsCopyTo {
        panel: Some(PanelId::Left),
        sources: vec!["abc.txt".into()],
        target: "out".into(),
        options: JobOptions::default(),
        queue: Some(held.id),
    };
    harness.app.engine().dispatch(copy).await.expect("queued");
    harness.press("Ctrl+Shift+Q").await;
    let show = QueueMsg::Lists(ListMsg::Show(held.id));
    harness.send(Message::Queue(show)).await;
    harness.press("Ctrl+A").await;
    harness.press("Shift+F10").await;
    let window = harness.app.queue_window.as_ref().expect("the window");
    let menu = window.menu.as_ref().expect("the menu");
    let clear = menu
        .items
        .iter()
        .position(|item| item.label == "Clear this list")
        .expect("Clear this list");
    {
        let layer = harness.app.queue_layer(nothing(), theme::tokens(false));
        let mut screen = Screen::new(layer).await;
        let stops = walk_round(&mut screen, "queue window with its menu");
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "queue window with its menu");
    }
    let clear = QueueMsg::Menu(MenuMsg::Pick(clear));
    harness.send(Message::Queue(clear)).await;
    let window = harness.app.queue_window.as_ref().expect("the window");
    assert!(window.confirm.is_some(), "a waiting job: the question");
    let layer = harness.app.queue_layer(nothing(), theme::tokens(false));
    let mut screen = Screen::new(layer).await;
    let stops = walk_round(&mut screen, "queue window asking");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "queue window asking");
    // The job list carries the question's text too: the button is the stop to press.
    let yes = stops
        .iter()
        .position(|seen| seen.kind == Some(Kind::Button) && seen.text.contains("Clear"))
        .expect("the Clear button");
    let yes = press_at(&mut screen, yes, Named::Enter);
    assert!(matches!(yes[..], [Message::Queue(QueueMsg::Confirm)]));
}

#[tokio::test]
async fn the_database_dialog_walks_its_modes_path_switch_and_buttons() {
    let (_root, mut harness) = harness().await;
    harness.send(Message::Database(DatabaseMsg::Open)).await;
    let dialog = harness.app.database.as_ref().expect("the dialog");
    let mut screen = Screen::new(database_dialog::overlay(dialog, theme::tokens(false))).await;
    let stops = walk_round(&mut screen, "database");
    assert_eq!(
        kinds(&stops)[..2],
        [Some(Kind::Choice), None],
        "the modes, the path"
    );
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "database");
    let mode = press_at(&mut screen, 0, Named::ArrowRight);
    assert!(matches!(
        mode[..],
        [Message::Database(DatabaseMsg::Mode(_))]
    ));
    let overwrite = press_on(&mut screen, "Replace the file", Named::Space);
    assert!(matches!(
        overwrite[..],
        [Message::Database(DatabaseMsg::Overwrite(_))]
    ));
    let close = press_on(&mut screen, "Close", Named::Enter);
    assert!(matches!(close[..], [Message::Database(DatabaseMsg::Close)]));
}

/// The hotlist editor ("Configure..." in Ctrl+D's menu) with one entry chosen: its list, its
/// fields, the entry's buttons and the dialog's.
#[tokio::test]
async fn the_hotlist_editor_walks_its_list_fields_and_buttons() {
    let (root, mut harness) = harness().await;
    harness.app.tabs.editor = Some(HotlistEditor::default());
    let entry = HotlistEntry {
        id: 1,
        parent_id: None,
        name: "home".to_owned(),
        folder: false,
        path: Some(VfsPath::local(root.path())),
        target: None,
    };
    harness
        .send(Message::Tabs(TabsMessage::HotlistRead(Ok(vec![entry]))))
        .await;
    harness.send(Message::Tabs(TabsMessage::Choose(0))).await;
    let layer = tabs::with_tabs_overlay(&harness.app, nothing(), theme::tokens(false));
    let mut screen = Screen::new(layer).await;
    let stops = walk_round(&mut screen, "hotlist editor");
    assert_eq!(stops[0].kind, Some(Kind::Choice), "the entries first");
    assert_eq!(
        kinds(&stops).iter().filter(|kind| kind.is_none()).count(),
        3,
        "name, folder, other panel's folder"
    );
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "hotlist editor");
    let add = press_on(&mut screen, "Add current folder", Named::Enter);
    assert!(matches!(add[..], [Message::Tabs(TabsMessage::AddCurrent)]));
    let close = press_on(&mut screen, "Close", Named::Enter);
    assert!(matches!(
        close[..],
        [Message::Tabs(TabsMessage::CloseEditor)]
    ));
}

/// A drive of the drive bar, `remote` for a network drive.
fn drive(id: &str, kind: DriveKind, remote: Option<&str>) -> Drive {
    Drive {
        id: id.to_owned(),
        group: DriveGroup::Drives,
        kind,
        label: id.to_owned(),
        root: Some(VfsPath::local(format!("{id}\\"))),
        state: DriveState::Unknown,
        volume: None,
        space: None,
        folder: Vec::new(),
        favourite: false,
        remote: remote.map(str::to_owned),
    }
}

/// Map and disconnect a network drive, as the drive menu opens them.
#[tokio::test]
async fn the_network_drive_dialogs_walk_their_controls() {
    let (_root, mut harness) = harness().await;
    let tokens = theme::tokens(false);
    let drives = vec![
        drive("C:", DriveKind::Fixed, None),
        drive("Z:", DriveKind::Network, Some(r"\\nas\backup")),
    ];
    harness
        .send(Message::Drive(DriveMessage::Listed(Ok(drives))))
        .await;
    let run = |action, drive: Option<&str>| {
        Message::Drive(DriveMessage::Run(
            PanelId::Left,
            action,
            drive.map(str::to_owned),
        ))
    };
    harness.send(run(DriveAction::MapNetwork, None)).await;
    {
        let mut screen = Screen::new(drive_bar::with_menu(&harness.app, nothing(), tokens)).await;
        let stops = walk_round(&mut screen, "map network drive");
        assert_eq!(
            kinds(&stops)[..3],
            [None, Some(Kind::Choice), Some(Kind::Toggle)],
            "the folder, the letter, reconnect"
        );
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "map network drive");
        let reconnect = press_on(&mut screen, "Reconnect at sign-in", Named::Space);
        assert!(matches!(
            reconnect[..],
            [Message::Drive(DriveMessage::Network(
                NetworkMsg::Reconnect(_)
            ))]
        ));
        let cancel = press_on(&mut screen, "Cancel", Named::Enter);
        assert!(matches!(
            cancel[..],
            [Message::Drive(DriveMessage::Network(NetworkMsg::Close))]
        ));
    }
    harness
        .send(Message::Drive(DriveMessage::Network(NetworkMsg::Close)))
        .await;
    harness
        .send(run(DriveAction::UnmapNetwork, Some("Z:")))
        .await;
    let mut screen = Screen::new(drive_bar::with_menu(&harness.app, nothing(), tokens)).await;
    let stops = walk_round(&mut screen, "disconnect network drive");
    assert_eq!(
        kinds(&stops),
        [Some(Kind::Choice), Some(Kind::Button), Some(Kind::Button)],
        "the drives, Disconnect, Cancel"
    );
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    // The drives' texts run up to Disconnect: it is pressed by its place.
    let disconnect = press_at(&mut screen, 1, Named::Enter);
    assert!(matches!(
        disconnect[..],
        [Message::Drive(DriveMessage::Network(NetworkMsg::Confirm))]
    ));
}

/// The question before a large fetch of remote entries dragged out (RC-080-01).
#[tokio::test]
async fn the_fetch_question_walks_download_and_cancel() {
    let (_root, mut harness) = beside_a_file().await;
    harness.app.ask_fetch(PanelId::Left, vec!["out".to_owned()]);
    let mut screen = Screen::new(harness.app.drag_layer(nothing(), theme::tokens(false))).await;
    let stops = walk_round(&mut screen, "fetch question");
    assert_eq!(kinds(&stops), [Some(Kind::Button); 2], "Download, Cancel");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    let download = press_on(&mut screen, "Download", Named::Enter);
    assert!(matches!(
        download[..],
        [Message::Drag(DragMsg::Fetch(true))]
    ));
    let cancel = press_on(&mut screen, "Cancel", Named::Space);
    assert!(matches!(cancel[..], [Message::Drag(DragMsg::Fetch(false))]));
}
