//! The folder trees through `update()` (RC-100-88): Ctrl+F8, the other panel following the
//! cursor, the keys of a tree, the separate trees and the tree dialog.

use std::path::{Path, PathBuf};

use rc_engine::{Chord, Command, PanelId, PanelView, VfsPath, tree::SeparateTree};
use tempfile::TempDir;

use super::{TreeId, TreeMsg, keys::passes};
use crate::app::{Message, cmdline::CmdLineMsg, harness::Harness};

/// A folder holding `apple/`, `berry/` and a file, by a name no tree hides.
fn folders() -> TempDir {
    let root = tempfile::Builder::new()
        .prefix("rc-tree-")
        .tempdir()
        .expect("temp dir");
    for name in ["apple", "berry"] {
        std::fs::create_dir(root.path().join(name)).expect("mkdir");
    }
    std::fs::write(root.path().join("file.txt"), "x").expect("write");
    root
}

/// `path` as the file system writes it: no link on the way (macOS' `/var`), no short name
/// (Windows' `RUNNER~1`), no verbatim prefix.
fn real(path: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(path).expect("canonical");
    let text = canonical.to_string_lossy();
    PathBuf::from(text.strip_prefix(r"\\?\").unwrap_or(&text))
}

/// Both panels on `root`, hidden folders shown so no folder on the way to it is left out.
async fn harness(root: &Path) -> Harness {
    let real = real(root);
    let mut harness = Harness::open(&real, &real).await;
    harness
        .app
        .engine()
        .dispatch(Command::ToggleHidden { show: Some(true) })
        .await
        .expect("shown");
    harness.settle().await;
    harness
}

fn cursor(harness: &Harness, id: TreeId) -> Option<VfsPath> {
    let ui = &harness.app.tree;
    let tree = match id {
        TreeId::Panel(panel) => ui.panels[panel.index()].as_ref().map(|kept| &kept.tree),
        TreeId::Separate(panel) => ui.separate[panel.index()].as_ref().map(|kept| &kept.tree),
        TreeId::Dialog => ui.dialog.as_ref().map(|dialog| &dialog.tree),
    };
    tree.and_then(|tree| tree.cursor_path().cloned())
}

fn path(harness: &Harness, panel: PanelId) -> VfsPath {
    harness.app.pane(panel).state.path.clone()
}

async fn typed(harness: &mut Harness, text: &str) {
    let chord: Chord = text.parse().expect("a chord");
    harness
        .send(Message::CmdLine(CmdLineMsg::Typed(chord, text.to_owned())))
        .await;
}

#[tokio::test]
async fn ctrl_f8_shows_the_tree_unfolded_to_the_panel_s_folder() {
    let root = folders();
    let mut harness = harness(root.path()).await;
    let here = VfsPath::local(real(root.path()));
    harness.press("Ctrl+F8").await;
    assert_eq!(harness.app.pane(PanelId::Left).state.view, PanelView::Tree);
    let id = TreeId::Panel(PanelId::Left);
    assert_eq!(cursor(&harness, id), Some(here.clone()));
    harness.press("ArrowDown").await;
    assert_eq!(cursor(&harness, id), Some(here.join("apple")), "no file");
    harness.press("Ctrl+F8").await;
    assert_eq!(harness.app.pane(PanelId::Left).state.view, PanelView::Files);
    assert!(harness.app.tree.panels[0].is_none());
}

#[tokio::test]
async fn the_other_panel_follows_after_the_pause_or_on_enter() {
    let root = folders();
    let mut harness = harness(root.path()).await;
    let here = VfsPath::local(real(root.path()));
    harness.press("Ctrl+F8").await;
    let id = TreeId::Panel(PanelId::Left);
    harness.press("ArrowDown").await;
    assert_eq!(path(&harness, PanelId::Right), here, "not before the pause");
    // The first move of the cursor is turn 1; its pause ended.
    harness.send(Message::Tree(TreeMsg::Rested(id, 1))).await;
    assert_eq!(path(&harness, PanelId::Right), here.join("apple"));
    // With `tree.open_on_enter` a pause leads nowhere, Enter does.
    harness
        .send(Message::Tree(TreeMsg::Settings(SeparateTree::Off, true)))
        .await;
    harness.press("ArrowDown").await;
    harness.send(Message::Tree(TreeMsg::Rested(id, 2))).await;
    assert_eq!(path(&harness, PanelId::Right), here.join("apple"));
    harness.press("Enter").await;
    assert_eq!(path(&harness, PanelId::Right), here.join("berry"));
}

#[tokio::test]
async fn letters_jump_and_ctrl_f1_ends_the_tree() {
    let root = folders();
    let mut harness = harness(root.path()).await;
    let here = VfsPath::local(real(root.path()));
    harness.press("Ctrl+F8").await;
    typed(&mut harness, "b").await;
    assert_eq!(
        cursor(&harness, TreeId::Panel(PanelId::Left)),
        Some(here.join("berry"))
    );
    harness.press("Ctrl+F1").await;
    let left = &harness.app.pane(PanelId::Left).state;
    assert_eq!(
        (left.view, left.mode),
        (PanelView::Files, rc_engine::ListMode::Brief)
    );
    assert!(harness.app.tree.panels[0].is_none());
}

#[test]
fn a_tree_runs_the_keys_that_change_the_view_not_those_on_entries() {
    let command = |id: &str, params: serde_json::Value| -> Command {
        serde_json::from_value(serde_json::json!({ "command": id, "params": params }))
            .expect("a command")
    };
    for kept in [
        command("panel.show_tree", serde_json::json!({})),
        command("panel.switch_active", serde_json::json!({})),
        command("panel.set_view", serde_json::json!({ "mode": "brief" })),
        command("app.quit", serde_json::json!({})),
    ] {
        assert!(passes(&kept), "{kept:?}");
    }
    for dropped in [
        command("ops.dialog", serde_json::json!({ "dialog": "copy" })),
        command("ops.dialog", serde_json::json!({ "dialog": "delete" })),
        command("panel.select_all", serde_json::json!({})),
    ] {
        assert!(!passes(&dropped), "{dropped:?}");
    }
}

#[tokio::test]
async fn shift_tab_gives_the_separate_tree_the_keys_and_it_leads_its_panel() {
    let root = folders();
    let mut harness = harness(root.path()).await;
    let here = VfsPath::local(real(root.path()));
    harness
        .send(Message::Tree(TreeMsg::Settings(SeparateTree::Two, false)))
        .await;
    let id = TreeId::Separate(PanelId::Left);
    assert_eq!(cursor(&harness, id), Some(here.clone()));
    assert!(harness.app.tree.separate[1].is_some(), "one beside each");
    harness.press("Shift+Tab").await;
    assert!(harness.app.tree.focus);
    harness.press("ArrowDown").await;
    harness.press("Enter").await;
    assert_eq!(path(&harness, PanelId::Left), here.join("apple"));
    assert_eq!(cursor(&harness, id), Some(here.join("apple")), "it follows");
    harness.press("Shift+Tab").await;
    assert!(!harness.app.tree.focus);
    harness
        .send(Message::Tree(TreeMsg::Settings(SeparateTree::Off, false)))
        .await;
    assert!(harness.app.tree.separate.iter().all(Option::is_none));
}

#[tokio::test]
async fn alt_f10_opens_the_dialog_and_enter_sends_the_panel_there() {
    let root = folders();
    let mut harness = harness(root.path()).await;
    let here = VfsPath::local(real(root.path()));
    harness.press("Alt+F10").await;
    assert_eq!(cursor(&harness, TreeId::Dialog), Some(here.clone()));
    harness.press("Escape").await;
    assert!(harness.app.tree.dialog.is_none());
    assert_eq!(path(&harness, PanelId::Left), here);
    harness.press("Alt+F10").await;
    typed(&mut harness, "b").await;
    harness.press("Enter").await;
    assert!(harness.app.tree.dialog.is_none());
    assert_eq!(path(&harness, PanelId::Left), here.join("berry"));
}
