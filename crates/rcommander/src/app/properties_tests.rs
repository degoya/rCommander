//! The properties dialogs through `apply`: the keys open them on the entry under the cursor,
//! their commands run on a real engine, the `..` row has none of them.

use std::{fs, path::Path, time::Duration};

use rc_engine::{DirSize, LinkKind, PanelId};
use tempfile::TempDir;

use super::{DialogKind, Phase, PropsMsg};
use crate::app::{Message, harness::Harness};

/// `root/` with `folder/` (one file of 10 bytes) and `notes.txt` (5 bytes).
fn tree() -> TempDir {
    let inside = "\0".repeat(10);
    crate::app::harness::tree(&[
        ("folder/inside.bin", inside.as_str()),
        ("notes.txt", "notes"),
    ])
}

/// Both panels on `root`, the cursor on the first entry (`folder`), not on `..`.
async fn harness(root: &Path) -> Harness {
    let mut harness = Harness::headless(root).await;
    harness.press("ArrowDown").await;
    harness
}

async fn send(harness: &mut Harness, message: PropsMsg) {
    harness.send(Message::Properties(message)).await;
}

fn dialog_kind(harness: &Harness) -> Option<DialogKind> {
    harness.app.properties.as_ref().map(|dialog| dialog.kind)
}

fn phase(harness: &Harness) -> Phase {
    harness
        .app
        .properties
        .as_ref()
        .map(|dialog| dialog.phase.clone())
        .expect("a dialog is open")
}

#[tokio::test]
async fn alt_enter_shows_the_properties_and_counts_on_demand() {
    let root = tree();
    let mut harness = harness(root.path()).await;
    harness.press("Alt+Enter").await;
    assert_eq!(dialog_kind(&harness), Some(DialogKind::Properties));
    assert_eq!(phase(&harness), Phase::Ready);
    let report = |harness: &Harness| {
        harness
            .app
            .properties
            .as_ref()
            .and_then(|dialog| dialog.report.clone())
            .expect("a report")
    };
    assert_eq!(report(&harness).items[0].entry.name, "folder");
    assert_eq!(report(&harness).items[0].usage, None);
    send(&mut harness, PropsMsg::Count).await;
    let usage = report(&harness).items[0].usage.expect("counted");
    assert_eq!((usage.bytes, usage.files), (10, 1));
    harness.press("Escape").await;
    assert_eq!(dialog_kind(&harness), None);
}

#[tokio::test]
async fn a_selection_is_summed() {
    let root = tree();
    let mut harness = harness(root.path()).await;
    harness.press("Ctrl+A").await;
    harness.press("Alt+Enter").await;
    let report = harness
        .app
        .properties
        .as_ref()
        .and_then(|dialog| dialog.report.clone())
        .expect("a report");
    assert_eq!(report.items.len(), 2);
    let total = report.total.expect("sums");
    assert_eq!((total.files, total.dirs), (1, 1));
}

#[tokio::test]
async fn the_parent_row_has_no_properties() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    assert!(harness.app.pane(PanelId::Left).on_parent());
    harness.press("Alt+Enter").await;
    assert_eq!(dialog_kind(&harness), None);
}

#[tokio::test]
async fn ctrl_l_shows_the_occupied_space() {
    let root = tree();
    let mut harness = harness(root.path()).await;
    harness.press("Ctrl+L").await;
    assert_eq!(dialog_kind(&harness), Some(DialogKind::Space));
    let space = harness
        .app
        .properties
        .as_ref()
        .and_then(|dialog| dialog.space)
        .expect("counted");
    assert_eq!(space.bytes, 10);
    harness.press("Enter").await;
    assert_eq!(dialog_kind(&harness), None);
}

#[tokio::test]
async fn ctrl_z_edits_the_comment_of_the_cursor_entry() {
    let root = tree();
    let mut harness = harness(root.path()).await;
    harness.press("ArrowDown").await;
    harness.press("Ctrl+Z").await;
    assert_eq!(dialog_kind(&harness), Some(DialogKind::Comment));
    send(&mut harness, PropsMsg::Comment("read me".into())).await;
    harness.press("Enter").await;
    assert_eq!(dialog_kind(&harness), None, "saved and closed");
    assert_eq!(
        fs::read_to_string(root.path().join("descript.ion")).expect("read"),
        "notes.txt read me\r\n"
    );
    harness.press("Ctrl+Z").await;
    let comment = harness
        .app
        .properties
        .as_ref()
        .map(|dialog| dialog.comment.clone());
    assert_eq!(
        comment.as_deref(),
        Some("read me"),
        "the dialog starts with it"
    );
}

#[tokio::test]
async fn ctrl_shift_f5_links_into_the_other_panel_and_greys_what_cannot_be_made() {
    let root = tree();
    let mut harness = harness(root.path()).await;
    harness.press("ArrowDown").await;
    harness.press("Ctrl+Shift+F5").await;
    assert_eq!(dialog_kind(&harness), Some(DialogKind::Link));
    let link = |harness: &Harness| {
        harness
            .app
            .properties
            .as_ref()
            .and_then(|dialog| dialog.link.clone())
            .expect("a link form")
    };
    let form = link(&harness);
    assert_eq!(form.target.path, root.path().join("notes.txt"));
    assert_eq!(form.kinds.junction, cfg!(windows));
    if !cfg!(windows) {
        send(&mut harness, PropsMsg::LinkKind(LinkKind::Junction)).await;
        assert_ne!(link(&harness).kind, LinkKind::Junction, "greyed out");
    }
    send(&mut harness, PropsMsg::LinkKind(LinkKind::Hardlink)).await;
    let place = root.path().join("second.txt").display().to_string();
    send(&mut harness, PropsMsg::LinkPath(place)).await;
    harness.press("Enter").await;
    assert_eq!(dialog_kind(&harness), None, "{:?}", harness.app.properties);
    assert_eq!(
        fs::read(root.path().join("second.txt")).expect("read"),
        b"notes"
    );
}

#[tokio::test]
async fn space_counts_the_folder_into_the_size_column() {
    let root = tree();
    let mut harness = harness(root.path()).await;
    harness.press("Space").await;
    for _ in 0..200 {
        harness.settle().await;
        let sizes = &harness.app.pane(PanelId::Left).state.dir_sizes;
        if sizes.get("folder") == Some(&DirSize::Bytes(10)) {
            assert_eq!(harness.app.pane(PanelId::Left).selected_count(), 1);
            assert_eq!(harness.app.pane(PanelId::Left).summary().bytes, 15);
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the folder was not counted");
}

#[tokio::test]
async fn ctrl_shift_c_copies_the_full_paths() {
    let root = tree();
    let mut harness = harness(root.path()).await;
    harness.press("Ctrl+Shift+C").await;
    assert!(
        harness
            .app
            .info
            .as_deref()
            .is_some_and(|info| info.contains('1')),
        "{:?}",
        harness.app.info
    );
}
