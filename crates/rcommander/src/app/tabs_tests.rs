use std::path::Path;

use rc_engine::{Command, Outcome, PanelId, Relocated, VfsPath};
use tempfile::TempDir;

use super::*;
use crate::app::harness::{self, Harness};

/// A folder with `a/deep` and `b`.
fn tree() -> TempDir {
    harness::tree(&[("a/deep/", ""), ("b/", "")])
}

async fn navigate(harness: &mut Harness, path: &str) {
    harness
        .app
        .engine()
        .dispatch(Command::Navigate {
            panel: Some(PanelId::Left),
            path: path.into(),
        })
        .await
        .expect("navigate");
    harness.settle().await;
}

fn shown(harness: &Harness) -> &Path {
    &harness.app.pane(PanelId::Left).state.path.path
}

#[tokio::test]
async fn ctrl_t_and_ctrl_w_change_the_tab_row_on_screen() {
    let root = tree();
    let mut harness = Harness::with_db(root.path()).await;
    assert_eq!(harness.app.tabs.strips[0].len(), 1);
    harness.press("Ctrl+T").await;
    assert_eq!(harness.app.tabs.strips[0].len(), 2);
    assert!(harness.app.tabs.strips[0][1].active);
    harness.press("Ctrl+W").await;
    assert_eq!(harness.app.tabs.strips[0].len(), 1);
}

#[tokio::test]
async fn a_click_shows_a_tab_and_a_middle_click_closes_it() {
    let root = tree();
    let mut harness = Harness::with_db(root.path()).await;
    harness.press("Ctrl+T").await;
    navigate(&mut harness, "a").await;
    let first = harness.app.tabs.strips[0][0].id;
    harness
        .send(Message::Tabs(TabsMessage::Select(PanelId::Left, first)))
        .await;
    assert_eq!(shown(&harness), root.path());
    let second = harness.app.tabs.strips[0][1].id;
    harness
        .send(Message::Tabs(TabsMessage::Close(PanelId::Left, second)))
        .await;
    assert_eq!(harness.app.tabs.strips[0].len(), 1);
}

/// `»` lists every tab with the shown one under the cursor; taking another shows it (RC-010-21).
#[tokio::test]
async fn the_list_of_all_tabs_shows_the_tab_taken() {
    let root = tree();
    let mut harness = Harness::with_db(root.path()).await;
    harness.press("Ctrl+T").await;
    navigate(&mut harness, "a").await;
    harness
        .send(Message::Tabs(TabsMessage::List(PanelId::Left)))
        .await;
    let popup = harness.app.tabs.popup.as_ref().expect("the list of tabs");
    assert_eq!(popup.rows.len(), 2);
    assert_eq!(popup.cursor, 1);
    harness.send(Message::Tabs(TabsMessage::Pick(0))).await;
    assert!(harness.app.tabs.popup.is_none());
    assert_eq!(shown(&harness), root.path());
    assert!(harness.app.tabs.strips[0][0].active);
}

/// Alt+Down lists the tab's folders newest first; Enter on an older one shows it.
#[tokio::test]
async fn alt_down_opens_the_history_and_enter_jumps() {
    let root = tree();
    let mut harness = Harness::with_db(root.path()).await;
    navigate(&mut harness, "a").await;
    navigate(&mut harness, "deep").await;
    harness.press("Alt+ArrowDown").await;
    let popup = harness.app.tabs.popup.as_ref().expect("the history");
    assert_eq!(popup.rows.len(), 3);
    assert_eq!(popup.cursor, 0, "the folder shown, the newest");
    harness.press("ArrowDown").await;
    harness.press("Enter").await;
    assert!(harness.app.tabs.popup.is_none());
    assert_eq!(shown(&harness), root.path().join("a"));
}

/// Ctrl+D opens the hotlist once it is read; "add current folder" adds the panel's folder.
#[tokio::test]
async fn ctrl_d_offers_to_add_the_current_folder() {
    let root = tree();
    let mut harness = Harness::with_db(root.path()).await;
    harness.press("Ctrl+D").await;
    // The window reads the hotlist off its thread; here the answer is handed in.
    harness
        .send(Message::Tabs(TabsMessage::HotlistRead(Ok(Vec::new()))))
        .await;
    assert!(harness.app.tabs.popup.is_some());
    harness.press("Enter").await;
    let listed = harness
        .app
        .engine()
        .dispatch(Command::ListHotlist {})
        .await
        .expect("list");
    let Outcome::Hotlist { items } = listed else {
        panic!("the hotlist");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].path, Some(VfsPath::local(root.path())));
}

/// A long history or hotlist is longer than its card: opening a popup and every move of its
/// cursor scroll the cursor into view (owner's Windows test, 2026-10-18).
#[tokio::test]
async fn a_popup_keeps_its_cursor_in_view() {
    let root = tree();
    let mut harness = Harness::with_db(root.path()).await;
    navigate(&mut harness, "a").await;
    navigate(&mut harness, "deep").await;
    let scrolls = |effects: Vec<Effect>| {
        effects
            .iter()
            .filter(|effect| matches!(effect, Effect::Task(_)))
            .count()
    };
    let key = |keys: &str| Message::Key(keys.parse().expect("chord"));
    assert_eq!(scrolls(harness.app.apply(key("Alt+ArrowDown"))), 1);
    for keys in ["ArrowDown", "End", "ArrowUp", "Home"] {
        assert_eq!(scrolls(harness.app.apply(key(keys))), 1, "{keys}");
    }
    assert_eq!(scrolls(harness.app.apply(key("Escape"))), 0);

    harness.press("Ctrl+D").await;
    let read = harness
        .app
        .apply(Message::Tabs(TabsMessage::HotlistRead(Ok(Vec::new()))));
    assert!(harness.app.tabs.popup.is_some());
    assert_eq!(scrolls(read), 1, "the hotlist opens scrolled to its cursor");
}

/// Ctrl+Up with the cursor on `..` opens the folder above in a new tab.
#[tokio::test]
async fn ctrl_up_on_the_parent_row_opens_the_folder_above() {
    let root = tree();
    let mut harness = Harness::with_db(root.path()).await;
    navigate(&mut harness, "a").await;
    assert!(harness.app.pane(PanelId::Left).on_parent());
    harness.press("Ctrl+ArrowUp").await;
    assert_eq!(harness.app.tabs.strips[0].len(), 2);
    assert_eq!(shown(&harness), root.path());
}

#[tokio::test]
async fn a_popup_and_the_dialog_have_the_keys_and_escape_closes_them() {
    let root = tree();
    let mut harness = Harness::with_db(root.path()).await;
    harness.press("Alt+ArrowDown").await;
    harness.press("Ctrl+T").await;
    assert_eq!(
        harness.app.tabs.strips[0].len(),
        1,
        "Ctrl+T went to the popup"
    );
    harness.press("Escape").await;
    assert!(harness.app.tabs.popup.is_none());
    harness.app.tabs.editor = Some(HotlistEditor::default());
    harness.press("Ctrl+T").await;
    assert_eq!(harness.app.tabs.strips[0].len(), 1);
    harness.press("Escape").await;
    assert!(harness.app.tabs.editor.is_none());
}

#[test]
fn a_relocated_tab_is_named_in_the_status_bar() {
    let one = Relocated {
        panel: PanelId::Left,
        stored: VfsPath::local("/gone/deep"),
        shown: VfsPath::local("/gone"),
    };
    assert_eq!(relocated_note(&[]), None);
    let note = relocated_note(std::slice::from_ref(&one)).expect("a note");
    assert!(note.contains("deep"), "{note}");
    let many = relocated_note(&[one.clone(), one]).expect("a note");
    assert!(many.contains('2'), "{many}");
    // A tab inside an archive whose file is gone: named through the archive file.
    let archive = VfsPath::local("/gone").join("backup.zip");
    let inside = Relocated {
        panel: PanelId::Left,
        stored: VfsPath::archive(archive, rc_vfs::ArchiveFormat::Zip, "/docs"),
        shown: VfsPath::local("/gone"),
    };
    let note = relocated_note(&[inside]).expect("a note");
    let through = Path::new("/gone").join("backup.zip").join("docs");
    assert!(note.contains(&through.display().to_string()), "{note}");
}

/// RC-080-16: Ctrl+Shift+A lists the panel's tabs, also with one tab; Enter shows the tab under
/// the cursor, Shift+F10 opens its menu in the list's place, Delete closes it.
#[tokio::test]
async fn ctrl_shift_a_lists_the_tabs_and_reaches_their_menu() {
    let root = tree();
    let mut harness = Harness::with_db(root.path()).await;
    harness.press("Ctrl+Shift+A").await;
    assert_eq!(
        harness
            .app
            .tabs
            .popup
            .as_ref()
            .map(|popup| popup.rows.len()),
        Some(1),
        "one tab is a list too"
    );
    harness.press("Escape").await;
    assert!(harness.app.tabs.popup.is_none());

    harness.press("Ctrl+T").await;
    harness.press("Ctrl+Shift+A").await;
    harness.press("Home").await;
    harness.press("Enter").await;
    assert!(
        harness.app.tabs.strips[0][0].active,
        "the first tab is shown"
    );

    harness.press("Ctrl+Shift+A").await;
    harness.press("End").await;
    harness.press("Shift+F10").await;
    let menu = harness.app.tabs.popup.as_ref().expect("the tab's menu");
    let second = harness.app.tabs.strips[0][1].id;
    assert_eq!(
        menu.rows.last().and_then(|row| row.action.clone()),
        Some(PopupAction::CloseTab(second))
    );
    harness.press("Escape").await;

    harness.press("Ctrl+Shift+A").await;
    harness.press("End").await;
    harness.press("Delete").await;
    assert_eq!(harness.app.tabs.strips[0].len(), 1);
}
