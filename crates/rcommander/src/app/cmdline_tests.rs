//! The command line, the quick search and the quick filter through the app and a real engine.

use iced::keyboard::{
    Key, Modifiers,
    key::{NativeCode, Physical},
};
use rc_engine::{Chord, PanelId, command_line::quote};
use tempfile::TempDir;

use super::{CmdLineMsg, Options, SearchKeys, SearchKind, key_message};
use crate::{
    app::{
        Message,
        harness::{self, Harness},
    },
    keymap,
};

/// `root/` with `inner/`, `outer/`, `my file.txt` and `f00.txt` to `f12.txt`.
fn tree() -> TempDir {
    let numbered: Vec<String> = (0..13).map(|i| format!("f{i:02}.txt")).collect();
    let mut entries = vec![("inner/", ""), ("outer/", ""), ("my file.txt", "x")];
    entries.extend(numbered.iter().map(|name| (name.as_str(), "x")));
    harness::tree(&entries)
}

/// Each character of `text` as the window reports a typed key.
async fn type_text(harness: &mut Harness, text: &str) {
    for c in text.chars() {
        let key: String = if c == ' ' {
            "Space".into()
        } else {
            c.to_lowercase().collect()
        };
        let mut chord = Chord::plain(&key).expect("a key");
        chord.shift = c.is_uppercase();
        harness
            .send(Message::CmdLine(CmdLineMsg::Typed(chord, c.to_string())))
            .await;
    }
}

/// AltGr with `key` as the window reports it on a German layout: no Ctrl, no Alt, the key with
/// its modifiers unidentified unless AltGr types `text` with it.
async fn altgr(harness: &mut Harness, key: &str, text: Option<&str>) {
    let modified = text.map_or(Key::Unidentified, |text| Key::Character(text.into()));
    let chord = keymap::pressed(
        &Key::Character(key.into()),
        &modified,
        Physical::Unidentified(NativeCode::Unidentified),
        Modifiers::empty(),
    )
    .expect("a chord");
    harness.send(key_message(chord, text)).await;
}

async fn search_keys(harness: &mut Harness, keys: SearchKeys) {
    let options = Options {
        keys,
        ..Options::default()
    };
    harness
        .send(Message::CmdLine(CmdLineMsg::Loaded(options, None)))
        .await;
}

fn line(harness: &Harness) -> &str {
    &harness.app.cmdline.text
}

fn cursor_name(harness: &Harness) -> String {
    let pane = harness.app.pane(PanelId::Left);
    pane.entries()[pane.cursor()].name.clone()
}

fn names(harness: &Harness) -> Vec<String> {
    let pane = harness.app.pane(PanelId::Left);
    pane.entries()
        .iter()
        .map(|entry| entry.name.clone())
        .collect()
}

#[tokio::test]
async fn letters_go_into_the_line_and_cd_moves_the_panel() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    type_text(&mut harness, "cd inner").await;
    assert_eq!(
        line(&harness),
        "cd inner",
        "the space too, once the line has text"
    );
    harness.press("Enter").await;
    assert_eq!(
        harness.app.pane(PanelId::Left).state.path.path,
        root.path().join("inner")
    );
    assert_eq!(line(&harness), "");
    assert_eq!(harness.app.cmdline.history, ["cd inner"]);
}

#[tokio::test]
async fn space_and_num_plus_keep_their_keys_on_an_empty_line() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("ArrowDown").await;
    type_text(&mut harness, " ").await;
    assert_eq!(line(&harness), "");
    assert_eq!(harness.app.pane(PanelId::Left).selected_count(), 1);
}

#[tokio::test]
async fn escape_and_backspace_edit_the_line() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    type_text(&mut harness, "dir").await;
    harness.press("Backspace").await;
    assert_eq!(line(&harness), "di");
    harness.press("Escape").await;
    assert_eq!(line(&harness), "");
    // On an empty line Backspace goes up again.
    let before = harness.app.pane(PanelId::Left).state.path.path.clone();
    harness.press("Backspace").await;
    assert_eq!(
        Some(harness.app.pane(PanelId::Left).state.path.path.as_path()),
        before.parent()
    );
}

#[tokio::test]
async fn ctrl_enter_ctrl_shift_enter_and_ctrl_p_insert_quoted() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    // `inner`, `outer`, then `f00.txt` ... `f12.txt`, `my file.txt`.
    harness.press("End").await;
    assert_eq!(cursor_name(&harness), "my file.txt");
    type_text(&mut harness, "notepad").await;
    harness.press("Ctrl+Enter").await;
    let name = quote("my file.txt").into_owned();
    assert_eq!(line(&harness), format!("notepad {name} "));

    harness.press("Escape").await;
    harness.press("Ctrl+Shift+Enter").await;
    let full = root.path().join("my file.txt");
    let full = quote(&full.to_string_lossy()).into_owned();
    assert_eq!(line(&harness), format!("{full} "));

    harness.press("Escape").await;
    harness.press("Ctrl+p").await;
    let folder = quote(&root.path().to_string_lossy()).into_owned();
    assert_eq!(line(&harness), format!("{folder} "));
}

#[tokio::test]
async fn ctrl_alt_letters_search_and_the_arrows_step_between_matches() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+Alt+f").await;
    assert_eq!(cursor_name(&harness), "f00.txt");
    harness.press("Ctrl+Alt+1").await;
    assert_eq!(cursor_name(&harness), "f10.txt");
    harness.press("ArrowDown").await;
    assert_eq!(cursor_name(&harness), "f11.txt");
    harness.press("ArrowDown").await;
    harness.press("ArrowDown").await;
    assert_eq!(cursor_name(&harness), "f12.txt", "no match beyond the last");
    harness.press("ArrowUp").await;
    assert_eq!(cursor_name(&harness), "f11.txt");
    // Plain letters continue an open search; one no name continues with is not taken.
    type_text(&mut harness, "z").await;
    let search = harness.app.cmdline.search.clone().expect("searching");
    assert_eq!(
        (search.kind, search.text.as_str()),
        (SearchKind::Find, "f1")
    );
    assert_eq!(line(&harness), "", "nothing went into the line");
    harness.press("Escape").await;
    assert!(harness.app.cmdline.search.is_none());
    assert_eq!(cursor_name(&harness), "f11.txt");
}

#[tokio::test]
async fn letters_alone_search_when_the_setting_says_so() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    search_keys(&mut harness, SearchKeys::Letters).await;
    type_text(&mut harness, "ou").await;
    assert_eq!(cursor_name(&harness), "outer");
    assert_eq!(line(&harness), "");
    harness.press("Ctrl+Alt+i").await;
    assert_eq!(
        cursor_name(&harness),
        "outer",
        "Ctrl+Alt is no search key now"
    );
    altgr(&mut harness, "i", None).await;
    assert_eq!(cursor_name(&harness), "outer", "nor is AltGr");
}

#[tokio::test]
async fn alt_letters_search_when_the_setting_says_so() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    search_keys(&mut harness, SearchKeys::Alt).await;
    harness.press("Alt+o").await;
    assert_eq!(cursor_name(&harness), "outer");
    harness.press("Escape").await;
    assert!(harness.app.cmdline.search.is_none());
    harness.press("Ctrl+Alt+i").await;
    altgr(&mut harness, "i", None).await;
    assert_eq!(
        cursor_name(&harness),
        "outer",
        "Ctrl+Alt and AltGr are no search keys now"
    );
    assert!(harness.app.cmdline.search.is_none());
    assert_eq!(line(&harness), "");
}

/// AltGr is Ctrl+Alt on Windows: with a letter it types nothing for, it searches as Ctrl+Alt
/// does; a character it types goes into the line (Windows test 2026-10-18).
#[tokio::test]
async fn altgr_letters_search_and_altgr_characters_are_typed() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    altgr(&mut harness, "q", Some("@")).await;
    assert_eq!(line(&harness), "@");
    assert!(harness.app.cmdline.search.is_none());
    harness.press("Escape").await;
    assert_eq!(line(&harness), "");
    altgr(&mut harness, "f", None).await;
    assert_eq!(cursor_name(&harness), "f00.txt");
    altgr(&mut harness, "1", None).await;
    assert_eq!(cursor_name(&harness), "f10.txt");
    let search = harness.app.cmdline.search.clone().expect("searching");
    assert_eq!(
        (search.kind, search.text.as_str()),
        (SearchKind::Find, "f1")
    );
    assert_eq!(line(&harness), "", "nothing went into the line");
}

/// `quick_search.filters` (owner, 2026-10-19): the quick search hides every file and folder its
/// text does not find; a character nothing continues with is not taken, Escape shows everything.
#[tokio::test]
async fn the_quick_search_filters_when_the_setting_says_so() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    let options = Options {
        filters: true,
        ..Options::default()
    };
    harness
        .send(Message::CmdLine(CmdLineMsg::Loaded(options, None)))
        .await;
    harness.press("Ctrl+Alt+i").await;
    assert_eq!(
        names(&harness),
        ["inner"],
        "a folder is filtered as a file is"
    );
    harness.press("Ctrl+Alt+z").await;
    assert_eq!(names(&harness), ["inner"], "z is not taken");
    harness.press("Backspace").await;
    assert_eq!(names(&harness).len(), 16);
    harness.press("Ctrl+Alt+f").await;
    harness.press("Ctrl+Alt+1").await;
    assert_eq!(names(&harness), ["f10.txt", "f11.txt", "f12.txt"]);
    let search = harness.app.cmdline.search.clone().expect("searching");
    assert_eq!(
        (search.kind, search.text.as_str()),
        (SearchKind::Filter, "f1")
    );
    harness.press("Escape").await;
    assert!(harness.app.cmdline.search.is_none());
    assert_eq!(names(&harness).len(), 16);
}

#[tokio::test]
async fn ctrl_s_filters_enter_keeps_it_and_escape_lifts_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+s").await;
    type_text(&mut harness, "f1*").await;
    assert_eq!(names(&harness), ["f10.txt", "f11.txt", "f12.txt"]);
    harness.press("Enter").await;
    assert!(harness.app.cmdline.search.is_none());
    assert_eq!(
        harness
            .app
            .pane(PanelId::Left)
            .state
            .filter
            .pattern
            .as_deref(),
        Some("f1*")
    );
    // The panel's keys work on the filtered list.
    harness.press("ArrowDown").await;
    assert_eq!(cursor_name(&harness), "f10.txt");
    harness.press("Escape").await;
    assert_eq!(names(&harness).len(), 16);
    harness.press("Ctrl+Shift+s").await;
    assert_eq!(names(&harness).len(), 3, "the last filter again");
}

#[tokio::test]
async fn escape_in_the_filter_lifts_it_and_backspace_widens_it() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+s").await;
    type_text(&mut harness, "f10").await;
    assert_eq!(names(&harness), ["f10.txt"]);
    harness.press("Backspace").await;
    assert_eq!(names(&harness), ["f10.txt", "f11.txt", "f12.txt"]);
    harness.press("Escape").await;
    assert!(harness.app.cmdline.search.is_none());
    assert_eq!(names(&harness).len(), 16);
}

#[tokio::test]
async fn the_history_comes_back_by_arrows_in_the_field_and_by_alt_f8() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    for typed in ["cd inner", "cd .."] {
        type_text(&mut harness, typed).await;
        harness.press("Enter").await;
    }
    assert_eq!(harness.app.cmdline.history, ["cd ..", "cd inner"]);

    // Without the keyboard in the field the arrows move the panel's cursor.
    let before = harness.app.pane(PanelId::Left).cursor_row();
    harness.press("ArrowDown").await;
    assert_ne!(harness.app.pane(PanelId::Left).cursor_row(), before);
    assert_eq!(line(&harness), "");

    // A click into the field gives it the keyboard.
    harness
        .send(Message::CmdLine(CmdLineMsg::Hover(true)))
        .await;
    harness.send(Message::CmdLine(CmdLineMsg::Press)).await;
    harness.press("ArrowUp").await;
    assert_eq!(line(&harness), "cd ..");
    harness.press("ArrowUp").await;
    assert_eq!(line(&harness), "cd inner");
    harness.press("ArrowDown").await;
    harness.press("ArrowDown").await;
    assert_eq!(line(&harness), "", "below the newest the line is empty");

    harness.press("Alt+F8").await;
    assert_eq!(harness.app.cmdline.list, Some(0));
    harness.press("ArrowDown").await;
    harness.press("Enter").await;
    assert_eq!(harness.app.cmdline.list, None);
    assert_eq!(line(&harness), "cd inner");
}
