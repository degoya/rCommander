//! `view.open` and Ctrl+Q through `apply`, without a window: the engine announces, the app opens
//! Lister or Quick View, and Quick View follows the cursor. Reading the file is a task the
//! harness does not run; `viewer/*_tests.rs` hold the reads. F3 is bound to `view.open` when
//! this branch meets RC-010-07's F-keys, so the tests dispatch the command itself.

use std::fs;

use rc_engine::{Command, EngineError, Outcome, PanelId, VfsPath, viewer::TextEncoding};
use tempfile::TempDir;

use super::{QuickShown, find::BarKind};
use crate::app::harness::{self, Harness};

/// `sub/`, then `a.txt` and `b.txt`.
fn tree() -> TempDir {
    harness::tree(&[("sub/", ""), ("a.txt", "alpha\n"), ("b.txt", "beta\n")])
}

/// `view.open` on the active panel, as F3 runs it; the app sees what the engine announces.
async fn view(harness: &mut Harness, path: Option<&str>) -> Result<Outcome, EngineError> {
    let command = Command::ViewFile {
        panel: None,
        path: path.map(str::to_owned),
    };
    let result = harness.app.engine().dispatch(command).await;
    harness.settle().await;
    result
}

fn top_path(harness: &Harness) -> Option<VfsPath> {
    harness
        .app
        .viewers
        .open
        .last()
        .map(|lister| lister.path.clone())
}

#[tokio::test]
async fn view_open_shows_lister_on_the_file_and_escape_closes_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    // `..`, then `sub`, then `a.txt`.
    harness.press("ArrowDown").await;
    harness.press("ArrowDown").await;
    view(&mut harness, None).await.expect("announced");
    assert_eq!(
        top_path(&harness),
        Some(VfsPath::local(root.path().join("a.txt")))
    );
    // Lister has the keys: ArrowDown scrolls it, the panel's cursor stays.
    harness.press("ArrowDown").await;
    let pane = harness.app.pane(PanelId::Left);
    assert_eq!(pane.entries()[pane.cursor()].name, "a.txt");
    harness.press("F7").await;
    assert_eq!(super::bar_of(&harness.app.viewers), Some(BarKind::Search));
    harness.press("Escape").await;
    assert_eq!(
        harness.app.viewers.open.len(),
        1,
        "Escape closes the bar first"
    );
    harness.press("Escape").await;
    assert!(harness.app.viewers.open.is_empty());
}

#[tokio::test]
async fn a_directory_opens_no_lister() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    let refused = view(&mut harness, Some("sub")).await;
    assert_eq!(refused.expect_err("a directory").code(), "is_a_directory");
    assert!(harness.app.viewers.open.is_empty());
}

#[tokio::test]
async fn n_and_p_step_to_the_next_file_and_move_the_cursor_along() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("ArrowDown").await;
    harness.press("ArrowDown").await;
    view(&mut harness, None).await.expect("announced");
    harness.press("N").await;
    assert_eq!(
        top_path(&harness),
        Some(VfsPath::local(root.path().join("b.txt")))
    );
    let pane = harness.app.pane(PanelId::Left);
    assert_eq!(pane.entries()[pane.cursor()].name, "b.txt");
    harness.press("N").await;
    assert_eq!(
        top_path(&harness),
        Some(VfsPath::local(root.path().join("b.txt"))),
        "no file after the last"
    );
    harness.press("P").await;
    assert_eq!(
        top_path(&harness),
        Some(VfsPath::local(root.path().join("a.txt"))),
        "back to the file before"
    );
}

#[tokio::test]
async fn ctrl_q_shows_the_cursor_in_the_other_panel_and_follows_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+Q").await;
    let quick = harness.app.viewers.quick.as_ref().expect("quick view on");
    assert_eq!(quick.target(), PanelId::Right);
    assert!(matches!(quick.shown, QuickShown::Nothing), "on `..`");
    harness.press("ArrowDown").await;
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    assert!(matches!(quick.shown, QuickShown::Dir { .. }));
    harness.press("ArrowDown").await;
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    assert_eq!(
        quick.path(),
        Some(&VfsPath::local(root.path().join("a.txt")))
    );
    assert!(matches!(quick.shown, QuickShown::File(_)));
    // The panel keeps its keys: the cursor moves, Quick View follows.
    harness.press("ArrowDown").await;
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    assert_eq!(
        quick.path(),
        Some(&VfsPath::local(root.path().join("b.txt")))
    );
    harness.press("Ctrl+Q").await;
    assert!(harness.app.viewers.quick.is_none());
}

/// RC-020-11: Quick View opens an `.nfo` as Lister does - in CP437 and without wrapping.
#[tokio::test]
async fn ctrl_q_shows_an_nfo_in_cp437_unwrapped() {
    let root = tree();
    fs::write(root.path().join("c.nfo"), b"\xC9\xCD\xBB\r\n").expect("write");
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+Q").await;
    // `..`, `sub`, `a.txt`, `b.txt`, then `c.nfo`.
    for _ in 0..4 {
        harness.press("ArrowDown").await;
    }
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    let QuickShown::File(lister) = &quick.shown else {
        panic!("the file is shown");
    };
    let (id, path) = (lister.id, lister.path.clone());
    assert_eq!(path, VfsPath::local(root.path().join("c.nfo")));
    // The harness runs no tasks: the answer of the open is handed over here.
    let opened = super::load::open(harness.app.engine().clone(), path, harness.app.db.clone())
        .await
        .expect("opens");
    harness
        .app
        .on_viewer(super::ViewerMessage::Opened(id, Ok(opened)));
    let lister = harness.app.viewers.lister(id).expect("still shown");
    assert_eq!(lister.detected.encoding, TextEncoding::Cp437);
    assert!(!lister.wrap);
}

/// RC-030-02: F3 on a text file inside a 7z opens Lister on the entry, which reads it through
/// the archive.
#[tokio::test]
async fn f3_inside_an_archive_opens_lister_on_the_entry() {
    let root = tree();
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../rc-vfs/tests/fixtures/archives/tree.7z");
    fs::copy(&fixture, root.path().join("tree.7z")).expect("copy");
    let mut harness = Harness::headless(root.path()).await;
    let engine = harness.app.engine().clone();
    let inside = root.path().join("tree.7z").join("docs");
    engine
        .dispatch(Command::Navigate {
            panel: None,
            path: inside.to_string_lossy().into_owned(),
        })
        .await
        .expect("into the archive");
    engine
        .dispatch(Command::SetCursor {
            panel: None,
            target: rc_engine::CursorTarget::Name("readme.md".into()),
        })
        .await
        .expect("cursor");
    harness.settle().await;
    harness.press("F3").await;
    harness.settle().await;
    let shown = top_path(&harness).expect("Lister is open");
    assert_eq!(rc_engine::shown_path(&shown), inside.join("readme.md"));
    let window = engine
        .open_for_view(&shown)
        .await
        .expect("read through the archive");
    let text = b"# Readme\n\nInside an archive.\n";
    assert_eq!(window.size(), u64::try_from(text.len()).expect("small"));
}
