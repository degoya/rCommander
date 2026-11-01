//! "Add key for a theme..." through `apply`: a theme chosen, its key recorded into the draft,
//! moved from another row after the question, removed again; Escape closes the panel.

use rc_db::Db;
use rc_engine::{Chord, settings::KEYMAP_KEY, settings::themes};
use tempfile::TempDir;

use super::super::{ShortcutsMsg, ShortcutsPage, page_message};
use super::*;
use crate::app::{
    Message,
    harness::Harness,
    settings::{self, Page, SettingsMessage, Tab, draft::Field},
};

/// The settings open on the Shortcuts tab, the window knowing the themes of an empty database.
async fn harness(root: &TempDir) -> Harness {
    let db = Db::open_in_memory().await.expect("db");
    let mut harness = Harness::on_db(root.path(), db.clone()).await;
    harness.app.db = Some(db.clone());
    crate::theme::themes::set_state(&themes::load(&db).await);
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

fn page(harness: &Harness) -> &ShortcutsPage {
    &harness
        .app
        .settings
        .as_ref()
        .expect("the settings")
        .shortcuts
}

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

async fn theme(harness: &mut Harness, message: ThemeKeyMsg) {
    harness
        .send(page_message(ShortcutsMsg::Theme(message)))
        .await;
}

fn classic() -> ThemeOption {
    let known = crate::app::palette::themes::known();
    options(&known)
        .into_iter()
        .find(|option| option.name == "theme:light:classic-light")
        .expect("the light Classic theme")
}

/// The acceptance's example: a key for "Theme (light): Classic", then taken away again.
#[tokio::test]
async fn a_key_for_a_theme_is_added_and_removed() {
    let root = tempfile::tempdir().expect("dir");
    let mut harness = harness(&root).await;
    theme(&mut harness, ThemeKeyMsg::Open).await;
    assert!(
        page(&harness)
            .adding
            .as_ref()
            .is_some_and(|adding| adding.name.is_some()),
        "a theme is chosen to begin with"
    );
    theme(&mut harness, ThemeKeyMsg::Pick(classic())).await;
    theme(&mut harness, ThemeKeyMsg::Record).await;
    assert!(page(&harness).adding.is_none());
    assert_eq!(page(&harness).recording, Some(Recording::Add));
    // The Enter that pressed the button comes back captured: no new key.
    harness.send(Message::KeyCaptured(chord("Enter"))).await;
    harness.press("Ctrl+Shift+1").await;
    assert_eq!(
        overrides(&harness),
        "Ctrl+Shift+1=theme:light:classic-light"
    );
    assert_eq!(
        page(&harness).selected,
        Some(Selected {
            name: "theme:light:classic-light".to_owned(),
            keys: "Ctrl+Shift+1".to_owned(),
        })
    );
    // Delete takes the key away, and the row with it.
    harness.press("Delete").await;
    assert_eq!(overrides(&harness), "");
}

/// A key another row holds asks first, as for any row; the yes moves it to the theme.
#[tokio::test]
async fn a_taken_key_asks_before_it_moves_to_the_theme() {
    let root = tempfile::tempdir().expect("dir");
    let mut harness = harness(&root).await;
    theme(&mut harness, ThemeKeyMsg::Open).await;
    theme(&mut harness, ThemeKeyMsg::Pick(classic())).await;
    theme(&mut harness, ThemeKeyMsg::Record).await;
    harness.press("F2").await;
    let asking = page(&harness).asking.as_ref().expect("the question");
    assert_eq!(asking.holder, "panel.refresh");
    harness.press("Enter").await;
    assert_eq!(overrides(&harness), "F2=theme:light:classic-light");
}

/// Escape closes the panel and leaves the dialog open; nothing is recorded.
#[tokio::test]
async fn escape_closes_the_panel() {
    let root = tempfile::tempdir().expect("dir");
    let mut harness = harness(&root).await;
    theme(&mut harness, ThemeKeyMsg::Open).await;
    harness.press("Escape").await;
    assert!(page(&harness).adding.is_none());
    assert!(page(&harness).recording.is_none());
    assert!(harness.app.settings.is_some(), "the dialog stays open");
    assert_eq!(overrides(&harness), "");
}

/// The list names every theme as the palette does, both modes.
#[test]
fn the_list_names_the_themes_as_the_palette_does() {
    let rows = vec![
        crate::app::palette::themes::ThemeRow {
            mode: ThemeMode::Light,
            id: "classic-light".to_owned(),
            name: "Classic".to_owned(),
            chosen: false,
        },
        crate::app::palette::themes::ThemeRow {
            mode: ThemeMode::Dark,
            id: "own-0a1b2c3d".to_owned(),
            name: "Evening".to_owned(),
            chosen: true,
        },
    ];
    let listed = options(&rows);
    assert_eq!(listed[1].name, "theme:dark:own-0a1b2c3d");
    assert_eq!(
        listed[1].to_string(),
        crate::app::palette::themes::row_label(ThemeMode::Dark, "Evening")
    );
    assert_eq!(
        first(&rows, true).as_deref(),
        Some("theme:dark:own-0a1b2c3d")
    );
    assert_eq!(
        first(&rows, false).as_deref(),
        Some("theme:light:classic-light"),
        "no light theme chosen: the first"
    );
}
