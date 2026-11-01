use rc_db::Db;
use rc_engine::{Chord, Keymap, Outcome, settings::themes};

use super::*;
use crate::app::{
    Message,
    harness::Harness,
    palette::{Palette, PaletteMsg, entries, reach},
};

fn row(mode: ThemeMode, id: &str, name: &str, chosen: bool) -> ThemeRow {
    ThemeRow {
        mode,
        id: id.to_owned(),
        name: name.to_owned(),
        chosen,
    }
}

/// A built-in theme of each mode and an own one, the dark Nord chosen.
fn listed() -> Vec<ThemeRow> {
    vec![
        row(ThemeMode::Light, "classic-light", "Classic", true),
        row(ThemeMode::Dark, "nord-dark", "Nord", true),
        row(ThemeMode::Dark, "own-0a1b2c3d", "Evening", false),
    ]
}

fn named<'a>(entries: &'a [Entry], name: &str) -> Option<&'a Entry> {
    entries.iter().find(|entry| entry.name == name)
}

fn chord(text: &str) -> Chord {
    text.parse().expect("a chord")
}

/// One row per theme and mode, named by mode and theme, running `theme.select`, the chosen ones
/// marked; a theme no longer listed has no row.
#[test]
fn the_rows_follow_the_list_of_themes() {
    let mut entries = entries(&Keymap::standard(), &reach::standard(), false);
    let before = entries.len();
    add_rows(&mut entries, &listed());
    assert_eq!(entries.len(), before + 3);
    let nord = named(&entries, "theme:dark:nord-dark").expect("a row");
    assert_eq!(nord.label, row_label(ThemeMode::Dark, "Nord"));
    assert_eq!(
        nord.action,
        Action::Run(Box::new(Command::SelectTheme {
            mode: ThemeMode::Dark,
            id: "nord-dark".to_owned(),
        }))
    );
    assert!(nord.current);
    let own = named(&entries, "theme:dark:own-0a1b2c3d").expect("an own theme's row");
    assert!(!own.current);
    assert!(named(&entries, "theme:light:classic-light").is_some_and(|entry| entry.current));
    let labels: Vec<String> = entries
        .iter()
        .map(|entry| entry.label.to_lowercase())
        .collect();
    assert!(labels.is_sorted(), "sorted by label");

    // The own theme deleted: the palette opened next has no row for it.
    let mut entries = super::super::entries(&Keymap::standard(), &reach::standard(), false);
    add_rows(&mut entries, &listed()[..2]);
    assert!(named(&entries, "theme:dark:own-0a1b2c3d").is_none());
}

/// The search finds the rows by "theme" and by the theme's name.
#[test]
fn the_search_finds_a_theme_by_word_and_name() {
    let mut entries = entries(&Keymap::standard(), &reach::standard(), false);
    add_rows(&mut entries, &listed());
    let mut palette = Palette::new(entries);
    palette.set_query("theme nord".into());
    let shown: Vec<&str> = palette
        .shown
        .iter()
        .filter_map(|index| palette.entry(*index))
        .map(|entry| entry.name.as_str())
        .collect();
    assert_eq!(shown, ["theme:dark:nord-dark"]);
}

/// A theme with a key of its own is that key's row: it runs the key, shows it and is marked.
#[test]
fn a_theme_with_a_key_is_the_row_of_its_key() {
    let mut keymap = Keymap::standard();
    let name = rc_engine::theme_key_name(ThemeMode::Dark, "nord-dark");
    keymap
        .assign(chord("Ctrl+Shift+1"), &name)
        .expect("a free key");
    let mut entries = entries(&keymap, &reach::standard(), false);
    add_rows(&mut entries, &listed());
    let rows: Vec<&Entry> = entries.iter().filter(|entry| entry.name == name).collect();
    let [nord] = rows.as_slice() else {
        panic!("one row: {rows:?}");
    };
    assert_eq!(nord.action, Action::Press(chord("Ctrl+Shift+1")));
    assert_eq!(nord.label, row_label(ThemeMode::Dark, "Nord"));
    assert!(nord.current);
    assert!(!nord.keys.is_empty());
}

/// A key's theme deleted: the key names its id and is marked missing.
#[test]
fn a_deleted_theme_is_named_by_its_id() {
    let themes = listed();
    assert_eq!(
        label_in("theme:dark:own-0a1b2c3d", &themes).as_deref(),
        Some(row_label(ThemeMode::Dark, "Evening").as_str())
    );
    assert_eq!(
        label_in("theme:dark:own-ffffffff", &themes).as_deref(),
        Some(row_label(ThemeMode::Dark, "own-ffffffff").as_str())
    );
    assert!(missing("theme:dark:own-ffffffff", &themes));
    // Of the other mode it is no theme of the key's.
    assert!(missing("theme:light:nord-dark", &themes));
    assert!(!missing("theme:dark:nord-dark", &themes));
    assert!(!missing("copy", &themes));
    assert_eq!(label_in("copy", &themes), None);
}

async fn on_themes() -> (tempfile::TempDir, Harness, Db) {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let harness = Harness::on_db(root.path(), db.clone()).await;
    crate::theme::themes::set_state(&themes::load(&db).await);
    (root, harness, db)
}

async fn chosen_dark(harness: &Harness) -> String {
    let Ok(Outcome::Themes(outcome)) = harness.app.engine().dispatch(Command::ListThemes {}).await
    else {
        panic!("the themes");
    };
    outcome
        .themes
        .into_iter()
        .find(|theme| theme.mode == ThemeMode::Dark && theme.selected)
        .expect("a dark theme chosen")
        .id
}

/// A theme's row picked in the palette runs `theme.select` through the window's `update()`.
#[tokio::test]
async fn a_picked_row_selects_the_theme() {
    let (_root, mut harness, _db) = on_themes().await;
    harness.press("Ctrl+Shift+P").await;
    let palette = harness.app.palette.as_ref().expect("open");
    let at = palette
        .shown
        .iter()
        .position(|index| {
            palette
                .entry(*index)
                .is_some_and(|entry| entry.name == "theme:dark:nord-dark")
        })
        .expect("a row for Nord");
    harness.send(Message::Palette(PaletteMsg::Pick(at))).await;
    assert!(harness.app.palette.is_none());
    assert_eq!(chosen_dark(&harness).await, "nord-dark");
}

/// A key whose theme is gone stays and says so when pressed, instead of doing nothing.
#[tokio::test]
async fn a_key_of_a_deleted_theme_says_so() {
    let (_root, mut harness, _db) = on_themes().await;
    let name = rc_engine::theme_key_name(ThemeMode::Dark, "own-ffffffff");
    harness
        .app
        .keymap
        .assign(chord("Ctrl+Shift+1"), &name)
        .expect("a free key");
    harness.press("Ctrl+Shift+1").await;
    assert_eq!(
        harness.app.pane(harness.app.active).error.as_deref(),
        Some(crate::i18n::error_text("theme_unknown").as_str())
    );
    // A key of a theme there is chooses it.
    let name = rc_engine::theme_key_name(ThemeMode::Dark, "nord-dark");
    harness
        .app
        .keymap
        .assign(chord("Ctrl+Shift+2"), &name)
        .expect("a free key");
    harness.press("Ctrl+Shift+2").await;
    assert_eq!(chosen_dark(&harness).await, "nord-dark");
}
