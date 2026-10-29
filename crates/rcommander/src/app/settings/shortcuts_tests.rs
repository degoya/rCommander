//! The Shortcuts tab through `apply`, against an engine over a database: a key recorded, added,
//! moved after the question, refused when the window keeps it, removed and reset; the draft's
//! text is what OK stores; and the keys in force reach the palette and the menu bar at once.

use rc_db::Db;
use rc_engine::{Chord, Command, Keymap, settings::KEYMAP_KEY};
use tempfile::TempDir;

use super::{Recording, Selected, ShortcutsMsg, page_message, shown};
use crate::app::{
    Message,
    harness::Harness,
    palette::{entries, menu_bar, reach},
    settings::{self, Page, SettingsMessage, Tab, draft::Field},
};
use crate::platform::menu_bar::BarItem;

/// The settings open on the Keyboard page's Shortcuts tab, read from an empty database.
async fn harness(root: &TempDir) -> Harness {
    let db = Db::open_in_memory().await.expect("db");
    let mut harness = Harness::on_db(root.path(), db.clone()).await;
    harness.app.db = Some(db.clone());
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let loaded = settings::load(harness.app.engine().clone(), db).await;
    harness
        .send(Message::Settings(SettingsMessage::Loaded(
            loaded.map(Box::new),
        )))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Keyboard)))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Tab(Tab::Shortcuts)))
        .await;
    harness
}

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

/// The draft's text of `keys.overrides`: what OK stores.
fn overrides(harness: &Harness) -> String {
    let draft = harness
        .app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.draft.as_ref())
        .expect("the draft");
    match draft.field(KEYMAP_KEY) {
        Some(Field::Text(text)) => text.clone(),
        other => panic!("a text, got {other:?}"),
    }
}

fn page(harness: &Harness) -> &super::ShortcutsPage {
    &harness
        .app
        .settings
        .as_ref()
        .expect("the settings")
        .shortcuts
}

fn note(harness: &Harness) -> Option<(String, bool)> {
    harness.app.settings.as_ref().and_then(|d| d.note.clone())
}

async fn select(harness: &mut Harness, name: &str, keys: &str) {
    let selected = Selected {
        name: name.to_owned(),
        keys: keys.to_owned(),
    };
    harness
        .send(page_message(ShortcutsMsg::Select(selected)))
        .await;
}

async fn record(harness: &mut Harness, recording: Recording, keys: &str) {
    harness
        .send(page_message(ShortcutsMsg::Record(recording)))
        .await;
    // The Enter that pressed the button comes back captured: no new key.
    harness.send(Message::KeyCaptured(chord("Enter"))).await;
    harness.press(keys).await;
}

/// The acceptance's example: F5 to Ctrl+K. The draft holds the deviation, the change is what
/// Apply stores, and the keymap read after it moves the copy in the palette and the menu bar.
#[tokio::test]
async fn a_recorded_key_replaces_the_rows_key_and_the_menus_follow() {
    let root = tempfile::tempdir().expect("dir");
    let mut harness = harness(&root).await;
    select(&mut harness, "copy", "F5").await;
    record(&mut harness, Recording::Change, "Ctrl+K").await;
    assert_eq!(overrides(&harness), "F5= Ctrl+K=copy");
    assert_eq!(
        page(&harness).selected.as_ref().map(|s| s.keys.as_str()),
        Some("Ctrl+K")
    );
    assert!(page(&harness).recording.is_none());
    assert!(harness.app.settings.is_some(), "the dialog stays open");
    let changes = harness
        .app
        .settings
        .as_ref()
        .and_then(|dialog| dialog.draft.as_ref())
        .map(|draft| draft.changes().expect("valid"))
        .expect("changes");
    assert_eq!(
        changes.settings,
        [(KEYMAP_KEY, serde_json::json!("F5= Ctrl+K=copy"))]
    );

    // What Apply runs, then the keymap the window reads after the engine's announcement.
    let engine = harness.app.engine().clone();
    engine
        .dispatch(Command::SetKeymap {
            overrides: "F5= Ctrl+K=copy".into(),
        })
        .await
        .expect("stored");
    let keymap = engine.keymap().await;
    harness
        .send(Message::Settings(SettingsMessage::Keymap(Box::new(keymap))))
        .await;
    let keymap = &harness.app.keymap;
    let rows = entries(keymap, &reach::standard(), false);
    let copy = rows
        .iter()
        .find(|entry| entry.name == "copy")
        .expect("copy");
    assert_eq!(copy.keys, "Ctrl+K");
    let menus = menu_bar::build(keymap, &rows);
    let shown = menus
        .iter()
        .flat_map(|menu| &menu.items)
        .find_map(|item| match item {
            BarItem::Entry { id: "copy", text } => Some(text.clone()),
            _ => None,
        })
        .expect("the menu's copy");
    assert!(shown.ends_with("\tCtrl+K"), "{shown}");
    assert_eq!(menu_bar::key_of(keymap, "copy").as_deref(), Some("Ctrl+K"));
}

#[tokio::test]
async fn a_second_key_is_added_to_the_row() {
    let root = tempfile::tempdir().expect("dir");
    let mut harness = harness(&root).await;
    select(&mut harness, "copy", "F5").await;
    record(&mut harness, Recording::Add, "Ctrl+K").await;
    assert_eq!(overrides(&harness), "Ctrl+K=copy");
    let keymap = super::keymap_of(
        harness
            .app
            .settings
            .as_ref()
            .and_then(|d| d.draft.as_ref())
            .expect("draft"),
    );
    let copy: Vec<String> = shown(&keymap, "copy")
        .into_iter()
        .filter(|row| row.name == "copy")
        .map(|row| row.keys)
        .collect();
    assert_eq!(copy, ["F5", "Ctrl+K"]);
}

/// A key another row holds asks first; Escape keeps it there, Enter moves it.
#[tokio::test]
async fn a_taken_key_asks_before_it_moves() {
    let root = tempfile::tempdir().expect("dir");
    let mut harness = harness(&root).await;
    select(&mut harness, "copy", "F5").await;
    record(&mut harness, Recording::Change, "F6").await;
    let asking = page(&harness).asking.clone().expect("the question");
    assert_eq!(asking.holder, "move");
    assert_eq!(overrides(&harness), "", "nothing yet");
    harness.press("Escape").await;
    assert!(page(&harness).asking.is_none());
    assert!(
        harness.app.settings.is_some(),
        "Escape left the dialog open"
    );
    assert_eq!(overrides(&harness), "");

    record(&mut harness, Recording::Change, "F6").await;
    harness.press("Enter").await;
    assert_eq!(overrides(&harness), "F5= F6=copy");
    assert!(harness.app.settings.is_some(), "Enter was the question's");
}

/// A key the window keeps is refused with its reason, and the recording waits for another;
/// F10 is recorded as a key, not taken as Escape; Escape ends the recording.
#[tokio::test]
async fn a_locked_key_is_refused_and_escape_ends_the_recording() {
    let root = tempfile::tempdir().expect("dir");
    let mut harness = harness(&root).await;
    select(&mut harness, "copy", "F5").await;
    record(&mut harness, Recording::Change, "Tab").await;
    let (text, error) = note(&harness).expect("a note");
    assert!(error && text.contains("Tab"), "{text}");
    assert_eq!(page(&harness).recording, Some(Recording::Change));
    assert!(harness.fields_walked.is_empty(), "Tab walked nothing");
    for keys in ["Alt+F", "Ctrl+V"] {
        harness.press(keys).await;
        assert_eq!(page(&harness).recording, Some(Recording::Change), "{keys}");
    }
    harness.press("F10").await;
    assert_eq!(
        page(&harness).asking.as_ref().map(|a| a.holder.as_str()),
        Some("app.quit"),
        "F10 is a key like any other here"
    );
    harness.press("Escape").await;
    record(&mut harness, Recording::Change, "Escape").await;
    assert!(page(&harness).recording.is_none());
    assert!(harness.app.settings.is_some());
    assert_eq!(overrides(&harness), "");
}

#[tokio::test]
async fn a_key_is_removed_and_rows_reset() {
    let root = tempfile::tempdir().expect("dir");
    let mut harness = harness(&root).await;
    select(&mut harness, "delete", "F8").await;
    harness.send(page_message(ShortcutsMsg::Remove)).await;
    assert_eq!(overrides(&harness), "F8=");
    assert_eq!(
        page(&harness).selected.as_ref().map(|s| s.keys.as_str()),
        Some("Delete"),
        "the row's other key"
    );
    harness.press("Delete").await;
    assert_eq!(overrides(&harness), "F8= Delete=");
    harness.send(page_message(ShortcutsMsg::Reset)).await;
    assert_eq!(overrides(&harness), "");

    select(&mut harness, "copy", "F5").await;
    record(&mut harness, Recording::Change, "Ctrl+K").await;
    select(&mut harness, "move", "F6").await;
    record(&mut harness, Recording::Add, "Ctrl+J").await;
    assert_eq!(overrides(&harness), "F5= Ctrl+J=move Ctrl+K=copy");
    harness.send(page_message(ShortcutsMsg::ResetAll)).await;
    assert_eq!(overrides(&harness), "");

    // A locked row keeps its key.
    select(&mut harness, "panel.switch_active", "Tab").await;
    harness.send(page_message(ShortcutsMsg::Remove)).await;
    assert_eq!(overrides(&harness), "");
    assert!(note(&harness).is_some_and(|(_, error)| error));
}

#[tokio::test]
async fn the_search_finds_by_name_or_key_and_the_arrows_select() {
    let root = tempfile::tempdir().expect("dir");
    let mut harness = harness(&root).await;
    let keymap = Keymap::standard();
    let by_key: Vec<String> = shown(&keymap, "shift+f6")
        .into_iter()
        .map(|row| row.name)
        .collect();
    assert_eq!(by_key, ["rename"]);
    let by_name = shown(&keymap, "COPY");
    assert!(by_name.iter().any(|row| row.name == "copy"), "{by_name:?}");
    assert!(shown(&keymap, "no such action anywhere").is_empty());

    harness
        .send(page_message(ShortcutsMsg::Search("shift+f6".into())))
        .await;
    harness.press("ArrowDown").await;
    assert_eq!(
        page(&harness).selected,
        Some(Selected {
            name: "rename".into(),
            keys: "Shift+F6".into()
        })
    );
}
