//! The context menu through `apply`: which entries a right click and Shift+F10 give it, the
//! cursor and selection they leave, rCommander's own menu and the Explorer's outcomes.

use std::{fs, path::Path};

use iced::Rectangle;
use rc_engine::{Chord, Command, CursorTarget, NameFormat, Outcome, PanelId, PanelView, VfsPath};
use rc_vfs::ArchiveFormat;
use tempfile::TempDir;

use super::{
    ContextMenu, ContextMessage, Target, is_local,
    menu::{Choice, Item},
};
use crate::{
    app::{
        Effect, Message,
        harness::{self, Harness},
    },
    platform::shell_menu::ShellOutcome,
};

/// `root/` with the directory `inner/` and the files `a.txt`, `b.txt`, `c.txt`; on screen `..`
/// is row 0, `inner` row 1, then the files in rows 2 to 4.
fn tree() -> TempDir {
    harness::tree(&[
        ("inner/", ""),
        ("a.txt", "x"),
        ("b.txt", "x"),
        ("c.txt", "x"),
    ])
}

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

fn names(names: &[&str]) -> Target {
    Target::Entries(names.iter().map(|name| (*name).to_owned()).collect())
}

fn open(harness: &Harness) -> &ContextMenu {
    harness.app.context_menu.as_ref().expect("an open menu")
}

fn selection(harness: &Harness, panel: PanelId) -> Vec<String> {
    let mut selected: Vec<String> = harness
        .app
        .pane(panel)
        .state
        .selection
        .iter()
        .cloned()
        .collect();
    selected.sort();
    selected
}

fn cursor_name(harness: &Harness, panel: PanelId) -> String {
    let pane = harness.app.pane(panel);
    pane.entries()[pane.cursor()].name.clone()
}

async fn right_click(harness: &mut Harness, panel: PanelId, row: usize) {
    harness
        .send(Message::ContextMenu(ContextMessage::Row(panel, row)))
        .await;
}

/// Selects `a.txt` and `b.txt` with Insert; the cursor ends on `c.txt`.
async fn select_a_and_b(harness: &mut Harness) {
    for keys in ["ArrowDown", "ArrowDown", "Insert", "Insert"] {
        harness.press(keys).await;
    }
}

const LIST: Rectangle = Rectangle {
    x: 8.0,
    y: 90.0,
    width: 600.0,
    height: 480.0,
};

#[tokio::test]
async fn a_right_click_beside_the_selection_drops_it_and_takes_the_clicked_entry() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    select_a_and_b(&mut harness).await;
    right_click(&mut harness, PanelId::Left, 4).await;
    assert_eq!(open(&harness).target, names(&["c.txt"]));
    assert!(!open(&harness).by_key);
    assert!(selection(&harness, PanelId::Left).is_empty());
    assert_eq!(cursor_name(&harness, PanelId::Left), "c.txt");
}

#[tokio::test]
async fn a_right_click_on_a_selected_entry_takes_the_whole_selection() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    select_a_and_b(&mut harness).await;
    right_click(&mut harness, PanelId::Left, 3).await;
    assert_eq!(open(&harness).target, names(&["a.txt", "b.txt"]));
    assert_eq!(selection(&harness, PanelId::Left), ["a.txt", "b.txt"]);
    assert_eq!(cursor_name(&harness, PanelId::Left), "b.txt");
}

#[tokio::test]
async fn a_right_click_on_the_parent_row_or_beside_the_rows_is_the_folder_s() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    right_click(&mut harness, PanelId::Left, 2).await;
    right_click(&mut harness, PanelId::Left, 0).await;
    assert_eq!(open(&harness).target, Target::Folder);
    assert!(harness.app.pane(PanelId::Left).on_parent());
    harness
        .send(Message::ContextMenu(ContextMessage::Beside(PanelId::Left)))
        .await;
    assert_eq!(open(&harness).target, Target::Folder);
    assert_eq!(open(&harness).row, 5, "one past the last row");
}

#[tokio::test]
async fn a_right_click_in_the_other_panel_makes_it_active() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    right_click(&mut harness, PanelId::Right, 2).await;
    assert_eq!(harness.app.active, PanelId::Right);
    assert_eq!(open(&harness).panel, PanelId::Right);
    assert_eq!(open(&harness).target, names(&["a.txt"]));
}

#[tokio::test]
async fn shift_f10_opens_the_menu_of_the_selection_at_the_cursor_s_row() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    select_a_and_b(&mut harness).await;
    harness.press("Shift+F10").await;
    let menu = open(&harness);
    assert_eq!(menu.target, names(&["a.txt", "b.txt"]));
    assert!(menu.by_key);
    assert_eq!(menu.row, harness.app.pane(PanelId::Left).cursor_row());
    assert_eq!(selection(&harness, PanelId::Left), ["a.txt", "b.txt"]);
}

#[tokio::test]
async fn the_menu_key_without_a_selection_takes_the_cursor_s_entry_or_the_folder() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("ContextMenu").await;
    assert_eq!(open(&harness).target, Target::Folder, "the cursor is on ..");
    harness.press("Escape").await;
    assert!(harness.app.context_menu.is_none());
    harness.press("ArrowDown").await;
    harness.press("ContextMenu").await;
    assert_eq!(open(&harness).target, names(&["inner"]));
}

#[tokio::test]
async fn rcommander_s_menu_offers_only_entries_whose_keys_run_a_command() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    right_click(&mut harness, PanelId::Left, 2).await;
    let items = &open(&harness).items;
    assert_eq!(
        items.first().and_then(|item| item.key()),
        Some(&chord("Enter"))
    );
    for key in items.iter().filter_map(|item| item.key()) {
        assert!(
            harness.app.keymap.lookup(key, PanelView::Files).is_some(),
            "{key} is not bound"
        );
    }
    harness
        .send(Message::ContextMenu(ContextMessage::Beside(PanelId::Left)))
        .await;
    let folder: Vec<&Chord> = open(&harness)
        .items
        .iter()
        .filter_map(|item| item.key())
        .collect();
    assert!(folder.contains(&&chord("Ctrl+R")));
}

#[tokio::test]
async fn the_menu_shows_once_measured_and_closes_when_the_list_is_gone() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    right_click(&mut harness, PanelId::Left, 2).await;
    assert_eq!(open(&harness).list, None, "not drawn before it is measured");
    harness
        .send(Message::ContextMenu(ContextMessage::Measured(Some(LIST))))
        .await;
    assert_eq!(open(&harness).list, Some(LIST));
    assert!(!open(&harness).shell, "no Explorer outside Windows");
    harness
        .send(Message::ContextMenu(ContextMessage::Measured(None)))
        .await;
    assert!(harness.app.context_menu.is_none());
}

#[tokio::test]
async fn enter_in_the_menu_presses_the_entry_s_key() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    right_click(&mut harness, PanelId::Left, 1).await;
    harness.press("Enter").await;
    assert!(harness.app.context_menu.is_none());
    let path = &harness.app.pane(PanelId::Left).state.path.path;
    assert_eq!(
        path,
        &root.path().join("inner"),
        "Open entered the directory"
    );
}

#[tokio::test]
async fn the_open_menu_swallows_the_panel_s_keys_and_escape_closes_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    right_click(&mut harness, PanelId::Left, 2).await;
    harness.press("Tab").await;
    assert_eq!(harness.app.active, PanelId::Left, "Tab went to the menu");
    harness.press("Escape").await;
    assert!(harness.app.context_menu.is_none());
    harness.press("Tab").await;
    assert_eq!(harness.app.active, PanelId::Right);
}

#[tokio::test]
async fn a_click_on_an_entry_runs_it_and_a_click_beside_closes() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    right_click(&mut harness, PanelId::Left, 2).await;
    harness
        .send(Message::ContextMenu(ContextMessage::Close))
        .await;
    assert!(harness.app.context_menu.is_none());
    right_click(&mut harness, PanelId::Left, 1).await;
    harness
        .send(Message::ContextMenu(ContextMessage::Pick(Choice::Key(
            chord("Enter"),
        ))))
        .await;
    assert!(harness.app.context_menu.is_none());
    let path = &harness.app.pane(PanelId::Left).state.path.path;
    assert_eq!(path, &root.path().join("inner"));
}

/// The menu of `a.txt` as it stood for the Explorer's.
/// The Explorer's menu for `a.txt` with rCommander's entries `choices` above it.
fn own_entries(choices: &[Choice]) -> Box<ContextMenu> {
    let mut menu = shell_menu();
    menu.items = choices
        .iter()
        .map(|choice| Item {
            label: "entry",
            choice: choice.clone(),
        })
        .collect();
    menu
}

fn shell_menu() -> Box<ContextMenu> {
    Box::new(ContextMenu {
        panel: PanelId::Left,
        target: names(&["a.txt"]),
        row: 2,
        by_key: false,
        items: Vec::new(),
        cursor: 0,
        list: Some(LIST),
        shell: true,
    })
}

#[tokio::test]
async fn after_a_shell_entry_ran_the_panel_is_read_again() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    let effects = harness
        .app
        .apply(Message::ContextMenu(ContextMessage::Shell(
            shell_menu(),
            ShellOutcome::Invoked,
        )));
    let refresh = vec![Command::Refresh {
        panel: Some(PanelId::Left),
    }];
    assert!(matches!(
        effects.as_slice(),
        [Effect::Dispatch(PanelId::Left, commands)] if *commands == refresh
    ));
    let dismissed = harness
        .app
        .apply(Message::ContextMenu(ContextMessage::Shell(
            shell_menu(),
            ShellOutcome::Dismissed,
        )));
    assert!(dismissed.is_empty());
    assert!(harness.app.context_menu.is_none());
}

#[tokio::test]
async fn when_the_shell_has_no_menu_rcommander_s_own_opens_in_its_place() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness
        .send(Message::ContextMenu(ContextMessage::Shell(
            shell_menu(),
            ShellOutcome::Failed,
        )))
        .await;
    let menu = open(&harness);
    assert!(!menu.shell);
    assert_eq!(menu.list, Some(LIST), "drawn at once where it stood");
    assert_eq!(
        menu.items.first().and_then(|item| item.key()),
        Some(&chord("Enter"))
    );
}

#[tokio::test]
async fn an_own_entry_of_the_shell_menu_presses_its_key() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("ArrowDown").await;
    harness
        .send(Message::ContextMenu(ContextMessage::Shell(
            own_entries(&[Choice::Key(chord("Enter"))]),
            ShellOutcome::Run(0),
        )))
        .await;
    let path = &harness.app.pane(PanelId::Left).state.path.path;
    assert_eq!(path, &root.path().join("inner"));
}

#[test]
fn only_a_local_folder_has_the_explorer_s_menu() {
    let local = VfsPath::local("/data/work");
    assert!(is_local(&local));
    let archive = VfsPath::archive(local, ArchiveFormat::Zip, "/docs");
    assert!(!is_local(&archive));
}

/// What the open menu's clipboard entry for `format` puts on the clipboard; the menu closes.
async fn copied(harness: &mut Harness, format: NameFormat) -> String {
    let choice = Choice::CopyNames(format);
    assert!(
        open(harness).items.iter().any(|item| item.choice == choice),
        "{format:?} is offered"
    );
    let effects = harness
        .app
        .apply(Message::ContextMenu(ContextMessage::Pick(choice)));
    assert!(harness.app.context_menu.is_none());
    for effect in effects {
        if let Effect::Properties(job) = effect {
            return match job.run(harness.app.engine().clone()).await {
                Ok(Outcome::ClipboardText { text }) => text,
                other => panic!("clipboard text, got {other:?}"),
            };
        }
    }
    panic!("no clipboard job");
}

#[tokio::test]
async fn the_menu_copies_the_name_or_full_path_of_the_clicked_entry() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    right_click(&mut harness, PanelId::Left, 2).await;
    assert_eq!(copied(&mut harness, NameFormat::Names).await, "a.txt");
    right_click(&mut harness, PanelId::Left, 3).await;
    assert_eq!(
        copied(&mut harness, NameFormat::FullPaths).await,
        root.path().join("b.txt").display().to_string()
    );
    // The answer reaches the clipboard and the status bar says so.
    right_click(&mut harness, PanelId::Left, 3).await;
    harness
        .send(Message::ContextMenu(ContextMessage::Pick(
            Choice::CopyNames(NameFormat::Names),
        )))
        .await;
    assert!(harness.app.info.is_some());
    // Beside the rows the menu is the folder's: nothing to copy.
    harness
        .send(Message::ContextMenu(ContextMessage::Beside(PanelId::Left)))
        .await;
    assert!(
        open(&harness)
            .items
            .iter()
            .all(|item| !matches!(item.choice, Choice::CopyNames(_)))
    );
}

#[tokio::test]
async fn the_menu_copies_the_whole_selection_one_line_each() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    select_a_and_b(&mut harness).await;
    right_click(&mut harness, PanelId::Left, 2).await;
    let text = copied(&mut harness, NameFormat::FullPaths).await;
    let lines: Vec<&str> = text.lines().collect();
    let expected: Vec<String> = ["a.txt", "b.txt"]
        .iter()
        .map(|name| root.path().join(name).display().to_string())
        .collect();
    assert_eq!(lines, expected);
}

#[tokio::test]
async fn inside_an_archive_the_full_path_is_the_one_the_panel_shows() {
    let root = tree();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../rc-vfs/tests/fixtures/archives/tree.zip");
    fs::copy(fixture, root.path().join("tree.zip")).expect("copy the fixture");
    let mut harness = Harness::headless(root.path()).await;
    let engine = harness.app.engine().clone();
    for command in [
        Command::Refresh { panel: None },
        Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("tree.zip".into()),
        },
        Command::OpenFile {
            panel: None,
            path: None,
        },
        Command::SetCursor {
            panel: None,
            target: CursorTarget::Name("hello.txt".into()),
        },
    ] {
        engine.dispatch(command).await.expect("into the archive");
    }
    harness.settle().await;
    harness.press("Shift+F10").await;
    let menu = open(&harness);
    assert!(!menu.shell, "the Explorer has no menu inside an archive");
    assert_eq!(menu.target, names(&["hello.txt"]));
    assert_eq!(
        copied(&mut harness, NameFormat::FullPaths).await,
        root.path()
            .join("tree.zip")
            .join("hello.txt")
            .display()
            .to_string()
    );
}

#[tokio::test]
async fn a_clipboard_entry_of_the_shell_menu_copies_too() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    let effects = harness
        .app
        .apply(Message::ContextMenu(ContextMessage::Shell(
            own_entries(&[Choice::CopyNames(NameFormat::Names)]),
            ShellOutcome::Run(0),
        )));
    assert!(
        effects
            .iter()
            .any(|effect| matches!(effect, Effect::Properties(_)))
    );
    let effects = harness
        .app
        .apply(Message::ContextMenu(ContextMessage::Shell(
            own_entries(&[]),
            ShellOutcome::Run(3),
        )));
    assert!(effects.is_empty(), "an index past the entries runs nothing");
}
