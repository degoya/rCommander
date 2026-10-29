//! The focus ring of the dialogs in windows of their own (RC-080-15, RC-080-16, RC-080-23): the
//! dialog drawn as its window draws it walks the same ring as its card, and leaves no button out
//! - the X is the window's title bar's, no button of the dialog's.

use std::time::Duration;

use iced::{Element, widget::Id};
use rc_engine::{ConflictChoice, ConnectionId, EngineEvent, PanelId};

use super::probe::{Screen, beside_a_file, harness, two_trees, walk_round};
use crate::{
    app::{
        Message, archive_ops, checksum, connections,
        database_dialog::{self, DatabaseMsg},
        dialog_windows::{DialogWindow, first_field},
        drive_bar::{self, DriveMessage, actions::DriveAction},
        file_ops::FileOpsMsg,
        properties, quick_connect, search,
        ssh_exec::{self, ExecMsg},
        tabs::{self, editor::HotlistEditor},
    },
    theme::{self, layout},
};

async fn walked(body: Element<'_, Message>, dialog: &str) -> (usize, usize) {
    let tokens = theme::tokens(false);
    let mut screen = Screen::new(layout::window_body(body, tokens)).await;
    let stops = walk_round(&mut screen, dialog);
    (stops.len(), screen.loose_buttons())
}

#[tokio::test]
async fn the_dialog_windows_walk_their_rings_without_a_head() {
    let (_root, mut harness) = harness().await;
    let tokens = theme::tokens(false);
    harness.press("Ctrl+F").await;
    let manager = harness.app.connections.as_ref().expect("the manager");
    let (stops, loose) = walked(connections::body(manager, tokens), "manager window").await;
    assert!(stops > 2, "the filter, the tree and the buttons");
    assert_eq!(loose, 0, "the title bar has the X");

    harness.press("Escape").await;
    harness.press("Alt+F7").await;
    let dialog = harness.app.search.as_ref().expect("the dialog");
    let (stops, loose) = walked(search::body(dialog, tokens), "find files window").await;
    assert!(stops > 2, "the tabs, the fields and the buttons");
    assert_eq!(loose, 0, "the title bar has the X");
}

/// One standalone dialog drawn as its window draws it (RC-080-23): it walks its ring without a
/// loose button - the title bar has the X - and its first field takes the keyboard.
async fn window_ring(body: Element<'_, Message>, kind: DialogWindow, dialog: &str) {
    let tokens = theme::tokens(false);
    let mut screen = Screen::new(layout::window_body(body, tokens)).await;
    walk_round(&mut screen, dialog);
    assert_eq!(
        screen.loose_buttons(),
        0,
        "{dialog}: the title bar has the X"
    );
    if let Some(field) = first_field(kind) {
        screen.focus(field);
        assert_eq!(
            screen.focused_id(),
            Some(Id::new(field)),
            "{dialog}: the first field has the keyboard"
        );
    }
}

#[tokio::test]
async fn the_standalone_dialog_windows_walk_their_rings_from_their_first_field() {
    let (_root, mut harness) = beside_a_file().await;
    let tokens = theme::tokens(false);

    harness.send(Message::Database(DatabaseMsg::Open)).await;
    let dialog = harness.app.database.as_ref().expect("the database dialog");
    let body = database_dialog::body(dialog, tokens);
    window_ring(body, DialogWindow::Database, "database window").await;
    harness.press("Escape").await;

    harness.press("Alt+Enter").await;
    let dialog = harness.app.properties.as_ref().expect("the properties");
    let body = properties::body(dialog, tokens);
    window_ring(body, DialogWindow::Properties, "properties window").await;
    harness.press("Escape").await;

    harness.press("Ctrl+N").await;
    let dialog = harness.app.quick.as_ref().expect("the quick connection");
    let body = quick_connect::body(dialog, tokens);
    window_ring(body, DialogWindow::QuickConnect, "quick connect window").await;
    harness.press("Escape").await;

    let map = DriveMessage::Run(PanelId::Left, DriveAction::MapNetwork, None);
    harness.send(Message::Drive(map)).await;
    let dialog = harness
        .app
        .drive_bar
        .dialog
        .as_ref()
        .expect("the network dialog");
    let body = drive_bar::network_body(dialog, tokens);
    window_ring(body, DialogWindow::Network, "network drive window").await;
    harness.press("Escape").await;

    harness.press("Alt+F5").await;
    let dialog = harness.app.archive.as_ref().expect("the pack dialog");
    let body = archive_ops::body(dialog, tokens);
    window_ring(body, DialogWindow::Archive, "pack window").await;
    harness.press("Escape").await;

    let create = EngineEvent::ChecksumDialog {
        panel: PanelId::Left,
        dialog: rc_engine::ChecksumDialog::Create,
    };
    harness.send(Message::Engine(create)).await;
    let form = harness.app.checksum.as_ref().expect("the checksum form");
    let body = checksum::body(form, tokens);
    window_ring(body, DialogWindow::ChecksumForm, "checksum window").await;
    harness.press("Escape").await;

    harness.app.tabs.editor = Some(HotlistEditor::default());
    let editor = harness
        .app
        .tabs
        .editor
        .as_ref()
        .expect("the hotlist dialog");
    let body = tabs::hotlist_body(editor, tokens);
    window_ring(body, DialogWindow::Hotlist, "hotlist window").await;
    harness.press("Escape").await;

    let open = ExecMsg::Open {
        connection: ConnectionId(7),
        name: "web".to_owned(),
    };
    harness.send(Message::SshExec(open)).await;
    let dialog = harness.app.ssh_exec.as_ref().expect("run command");
    let body = ssh_exec::body(dialog, tokens);
    window_ring(body, DialogWindow::SshExec, "run command window").await;
}

/// The file operations' windows (RC-080-23, stage 2) walk their rings without a loose button -
/// the dialogs of F5, F7 and F8 from their field -, and so do a job's conflict and its progress
/// window.
#[tokio::test]
async fn the_file_operations_windows_walk_their_rings() {
    let (_root, mut harness) = two_trees().await;
    harness.app.dialog_windows.own = true;
    let tokens = theme::tokens(false);
    for (key, kind) in [
        ("F5", DialogWindow::Transfer),
        ("F7", DialogWindow::NewItem),
        ("F8", DialogWindow::Delete),
    ] {
        harness.press(key).await;
        let body = harness.app.file_ops_body(kind, tokens).expect("drawn");
        window_ring(body, kind, key).await;
        harness.press("Escape").await;
    }

    harness.press("F5").await;
    harness
        .send(Message::FileOps(FileOpsMsg::Start { background: false }))
        .await;
    let engine = harness.app.engine().clone();
    let job = engine.ops().jobs().last().map(|job| job.id).expect("a job");
    for _ in 0..500 {
        if engine.ops().job(job).expect("known").question.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    harness.settle().await;
    for kind in [DialogWindow::Conflict, DialogWindow::Progress] {
        let body = harness.app.file_ops_body(kind, tokens).expect("drawn");
        window_ring(body, kind, "a job's window").await;
    }
    harness
        .send(Message::FileOps(FileOpsMsg::Conflict {
            job,
            choice: ConflictChoice::Cancel,
            for_all: false,
        }))
        .await;
}
