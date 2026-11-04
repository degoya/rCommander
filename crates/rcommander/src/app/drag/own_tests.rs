//! The system's drag of local entries (RC-080-01), without a window: which drop is our own, and
//! where it lands. The drag loop itself is Windows' and is accepted there.

use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

use iced::{event, keyboard::Modifiers, window};
use rc_engine::{Command, FileDialog, PanelId};
use tempfile::TempDir;

use super::OwnDrag;
use crate::{
    app::{
        Message,
        drag::{DragMsg, pointer_events},
        file_ops::FileOpsMsg,
        harness::Harness,
    },
    platform::drag_out::{DragOutcome, Pointer},
};

/// `root/` with `a.txt` and `out/`; the left panel on `root`, the right one on `out`, and the
/// system's drag of `a.txt` from the left panel going on.
async fn dragging() -> (TempDir, Harness, PathBuf) {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir(root.path().join("out")).expect("mkdir");
    let file = root.path().join("a.txt");
    fs::write(&file, b"a").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    harness
        .app
        .engine()
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Right),
            path: "out".into(),
        })
        .await
        .expect("dispatched");
    harness.settle().await;
    // The press the drag began with: the cursor goes to the row.
    let pane = harness.app.pane(PanelId::Left);
    let row = pane
        .entries()
        .iter()
        .position(|entry| entry.name == "a.txt")
        .expect("listed")
        + usize::from(pane.has_parent_row());
    harness.send(Message::Click(PanelId::Left, row)).await;
    harness.app.drag.own = Some(own(file.clone(), None));
    (root, harness, file)
}

fn own(file: PathBuf, ended: Option<Instant>) -> OwnDrag {
    OwnDrag {
        from: PanelId::Left,
        names: vec!["a.txt".to_owned()],
        paths: vec![file],
        ended,
    }
}

async fn drop_at(harness: &mut Harness, file: PathBuf, across: f32, shift: bool) {
    harness
        .send(Message::Drag(DragMsg::FileDropped(file)))
        .await;
    harness
        .send(Message::Drag(DragMsg::Settled(Some(Pointer {
            across,
            shift,
        }))))
        .await;
}

#[test]
fn only_the_dragged_files_soon_after_the_loop_are_our_own() {
    let file = PathBuf::from("/root/a.txt");
    let now = Instant::now();
    let running = own(file.clone(), None);
    assert!(running.takes(std::slice::from_ref(&file), now));
    assert!(!running.takes(&[], now), "nothing dropped");
    assert!(
        !running.takes(&[file.clone(), PathBuf::from("/elsewhere/b.txt")], now),
        "a file of another program's drop"
    );
    let just_over = own(file.clone(), Some(now));
    assert!(just_over.takes(std::slice::from_ref(&file), now + Duration::from_secs(1)));
    assert!(
        !just_over.takes(std::slice::from_ref(&file), now + Duration::from_secs(5)),
        "long after the loop it is another drop of the same file"
    );
}

#[tokio::test]
async fn our_drop_on_the_other_panel_opens_the_copy_dialog_and_copies_nothing_yet() {
    let (root, mut harness, file) = dragging().await;
    drop_at(&mut harness, file, 0.8, false).await;
    let dialog = harness
        .app
        .file_ops
        .dialog
        .clone()
        .expect("the copy dialog");
    assert_eq!(dialog.kind, FileDialog::Copy);
    assert_eq!(dialog.single.as_deref(), Some("a.txt"));
    assert!(
        !root.path().join("out/a.txt").exists(),
        "the dialog decides"
    );
    assert!(harness.app.engine().ops().jobs().is_empty(), "no job");
    assert!(harness.app.drag.own.is_none(), "the drag is over");
}

#[tokio::test]
async fn shift_at_our_drop_opens_the_move_dialog() {
    let (_root, mut harness, file) = dragging().await;
    drop_at(&mut harness, file, 0.8, true).await;
    let dialog = harness
        .app
        .file_ops
        .dialog
        .clone()
        .expect("the move dialog");
    assert_eq!(dialog.kind, FileDialog::Move);
}

#[tokio::test]
async fn our_drop_on_the_panel_it_came_from_does_nothing() {
    let (_root, mut harness, file) = dragging().await;
    drop_at(&mut harness, file, 0.2, false).await;
    assert!(harness.app.file_ops.dialog.is_none(), "no dialog");
    assert!(
        harness.app.engine().ops().jobs().is_empty(),
        "no copy into its own folder"
    );
}

#[tokio::test]
async fn a_drag_the_system_never_started_drops_where_the_pointer_is() {
    let (_root, mut harness, _file) = dragging().await;
    harness.send(Message::Hover(PanelId::Right)).await;
    harness
        .send(Message::FileOps(FileOpsMsg::Modifiers(Modifiers::empty())))
        .await;
    harness
        .send(Message::Drag(DragMsg::Ended {
            panel: PanelId::Left,
            outcome: DragOutcome::NotHeld,
            fetched: None,
        }))
        .await;
    let dialog = harness
        .app
        .file_ops
        .dialog
        .clone()
        .expect("the copy dialog");
    assert_eq!(dialog.kind, FileDialog::Copy);
}

/// Alt+Tab while dragging: the window losing the focus starts the system's drag as leaving it
/// does.
#[test]
fn losing_the_focus_counts_as_leaving_the_window() {
    let message = pointer_events(
        iced::Event::Window(window::Event::Unfocused),
        event::Status::Ignored,
        window::Id::unique(),
    );
    assert!(
        matches!(message, Some(Message::Drag(DragMsg::Left))),
        "{message:?}"
    );
}

/// The button's release that ends the system's drag reaches the window as a release without a
/// gesture: it is no click and leaves the marks the drag took, also where a click clears them.
#[tokio::test]
async fn the_release_that_ends_our_drag_is_no_click() {
    let (_root, mut harness, _file) = dragging().await;
    harness
        .app
        .engine()
        .dispatch(Command::ToggleSelection {
            panel: Some(PanelId::Left),
            names: vec!["a.txt".to_owned()],
        })
        .await
        .expect("dispatched");
    harness.settle().await;
    harness
        .send(Message::Drag(DragMsg::ClickClears(true)))
        .await;
    // The press on the marked row the drag starts with; the system's drag takes over.
    let pane = harness.app.pane(PanelId::Left);
    let row = pane
        .entries()
        .iter()
        .position(|entry| entry.name == "a.txt")
        .expect("listed")
        + usize::from(pane.has_parent_row());
    harness.send(Message::Click(PanelId::Left, row)).await;
    harness.app.drag.own = Some(own(PathBuf::from("/x/a.txt"), None));
    harness.send(Message::Drag(DragMsg::Released)).await;
    assert!(
        harness
            .app
            .pane(PanelId::Left)
            .state
            .selection
            .contains("a.txt"),
        "the marks stay"
    );
}
