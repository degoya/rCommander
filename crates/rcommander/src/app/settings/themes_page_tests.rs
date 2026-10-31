//! The Themes tab over the app's `update()`, without a window (RC-100-50): what it reads, the
//! question of an import's name, the editor opened, edited and closed.

use std::collections::BTreeMap;

use rc_db::Db;
use rc_engine::{
    Command,
    settings::themes::{ImportedTheme, ThemeAccent, ThemeDraft},
};
use serde_json::json;
use tempfile::TempDir;

use super::{Answer, ConflictChoice, EditorMsg, ThemesMsg, actions, concerns, editor::Slot, read};
use crate::app::{
    Message,
    harness::Harness,
    settings::{self, SettingsMessage},
};

fn themes(message: ThemesMsg) -> Message {
    Message::Settings(SettingsMessage::Themes(message))
}

/// The dialog open with its draft, the themes read.
async fn open(db: &Db) -> (TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    let mut harness = Harness::on_db(root.path(), db.clone()).await;
    harness.app.db = Some(db.clone());
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    let loaded = settings::load(harness.app.engine().clone(), db.clone()).await;
    harness
        .send(Message::Settings(SettingsMessage::Loaded(
            loaded.map(Box::new),
        )))
        .await;
    reread(&mut harness, db).await;
    (root, harness)
}

/// The themes read again, as the task after a change does.
async fn reread(harness: &mut Harness, db: &Db) {
    let read = read(db.clone()).await;
    harness.send(themes(ThemesMsg::Read(Box::new(read)))).await;
}

fn page(harness: &Harness) -> &super::ThemesPage {
    &harness.app.settings.as_ref().expect("the dialog").themes
}

async fn save(harness: &Harness, name: &str, dark: bool) -> String {
    let outcome = actions::run(
        harness.app.engine().clone(),
        Command::SaveTheme {
            theme: ThemeDraft {
                id: None,
                name: name.to_owned(),
                dark,
                accent: "system".to_owned(),
                tokens: BTreeMap::new(),
            },
        },
    )
    .await
    .expect("saved");
    outcome.saved.expect("its id")
}

#[test]
fn the_theme_and_colour_keys_concern_the_tab() {
    assert!(concerns("appearance.theme_light"));
    assert!(concerns("appearance.theme_dark"));
    assert!(concerns("appearance.colours.dark.cursor"));
    assert!(concerns("appearance.colour_rules"));
    assert!(
        concerns("appearance.connection_themes"),
        "the connections' themes switched (RC-100-51)"
    );
    assert!(!concerns("appearance.theme"));
}

/// The read takes RC-100-43's colours into a Custom theme, chosen; the tab lists it and marks
/// the theme of the window's mode.
#[tokio::test]
async fn the_read_adopts_old_colours_and_the_tab_lists_them() {
    let db = Db::open_in_memory().await.expect("db");
    db.set("appearance.colours.light.background", &json!("#fff8e0"))
        .await
        .expect("set");
    let (_root, harness) = open(&db).await;
    let state = page(&harness).state.as_ref().expect("read");
    let custom = state
        .themes
        .iter()
        .find(|theme| theme.name == "Custom (light)")
        .expect("the Custom theme");
    assert_eq!(state.chosen[0], custom.id);
    let marked = page(&harness).selected.as_deref();
    assert_eq!(
        marked,
        Some(state.chosen[usize::from(harness.app.dark)].as_str())
    );
}

/// An import whose name an own theme has asks; Rename takes a free name and saves, Cancel stops.
#[tokio::test]
async fn an_import_with_a_taken_name_asks_first() {
    let db = Db::open_in_memory().await.expect("db");
    let (_root, mut harness) = open(&db).await;
    save(&harness, "Night", true).await;
    reread(&mut harness, &db).await;
    let imported = || ImportedTheme {
        name: "NIGHT".to_owned(),
        dark: true,
        accent: ThemeAccent::System,
        tokens: BTreeMap::new(),
        dropped: vec!["sparkle".to_owned()],
    };
    harness
        .send(themes(ThemesMsg::Imported(Ok(Box::new(imported())))))
        .await;
    let conflict = page(&harness).conflict.as_ref().expect("asked");
    assert_eq!(conflict.existing.name, "Night");
    harness
        .send(themes(ThemesMsg::Conflict(ConflictChoice::Cancel)))
        .await;
    assert!(page(&harness).conflict.is_none());
    assert!(!page(&harness).busy, "nothing saved");

    harness
        .send(themes(ThemesMsg::Imported(Ok(Box::new(imported())))))
        .await;
    harness
        .send(themes(ThemesMsg::Conflict(ConflictChoice::Rename)))
        .await;
    assert!(page(&harness).conflict.is_none());
    assert!(page(&harness).busy, "the save runs");
}

/// A refused file is a note, not a theme.
#[tokio::test]
async fn a_refused_file_is_noted() {
    let db = Db::open_in_memory().await.expect("db");
    let (_root, mut harness) = open(&db).await;
    harness
        .send(themes(ThemesMsg::Imported(Err("theme_file_colour"))))
        .await;
    let (note, error) = page(&harness).note.clone().expect("a note");
    assert!(error);
    assert!(!note.is_empty());
    harness.send(themes(ThemesMsg::Import)).await;
    assert!(
        page(&harness)
            .note
            .as_ref()
            .is_some_and(|(_, error)| *error),
        "no file named"
    );
}

/// The editor over `update()`: opened on a theme made, a colour typed into the edited theme, a
/// half-typed one kept in its field only, Reset, Cancel closing it.
#[tokio::test]
async fn the_editor_edits_and_cancels() {
    let db = Db::open_in_memory().await.expect("db");
    let (_root, mut harness) = open(&db).await;
    let id = save(&harness, "Day", false).await;
    reread(&mut harness, &db).await;
    harness.send(themes(ThemesMsg::Choose(id.clone()))).await;
    harness.send(themes(ThemesMsg::Edit)).await;
    let editing = page(&harness).editor.as_ref().expect("the editor");
    assert_eq!(editing.theme.id, id);

    let edit = |message| themes(ThemesMsg::Editor(message));
    harness
        .send(edit(EditorMsg::Hex(
            Slot::Token("window"),
            "#102030".into(),
        )))
        .await;
    harness
        .send(edit(EditorMsg::Hex(Slot::Token("surface"), "#12".into())))
        .await;
    let editor = page(&harness).editor.as_ref().expect("the editor");
    assert_eq!(
        editor.theme.tokens.get("window").map(String::as_str),
        Some("#102030")
    );
    assert!(!editor.theme.tokens.contains_key("surface"));
    assert_eq!(
        editor
            .typed
            .get(&Slot::Token("surface"))
            .map(String::as_str),
        Some("#12")
    );
    harness.send(edit(EditorMsg::Reset("window"))).await;
    let editor = page(&harness).editor.as_ref().expect("the editor");
    assert!(
        !editor.theme.tokens.contains_key("window"),
        "back as it opened"
    );

    harness.send(edit(EditorMsg::Cancel)).await;
    assert!(page(&harness).editor.is_none());
}

/// The engine's answer to New opens the editor on the theme under the id it got; the editor's
/// save closes it.
#[tokio::test]
async fn an_answer_opens_and_closes_the_editor() {
    let db = Db::open_in_memory().await.expect("db");
    let (_root, mut harness) = open(&db).await;
    let id = save(&harness, "Fresh", false).await;
    let state = read(db.clone()).await.state;
    let fresh = state.theme(&id).cloned().expect("there");
    let answer = |edit, close_editor| Answer {
        result: Ok(Box::new(rc_engine::settings::themes::ThemesOutcome {
            themes: state.infos(),
            saved: Some(id.clone()),
        })),
        note: "done".to_owned(),
        edit,
        close_editor,
    };
    harness
        .send(themes(ThemesMsg::Answered(Box::new(answer(
            Some(Box::new(fresh.clone())),
            false,
        )))))
        .await;
    assert!(page(&harness).editor.is_some());
    assert_eq!(page(&harness).selected.as_deref(), Some(id.as_str()));
    harness
        .send(themes(ThemesMsg::Answered(Box::new(answer(None, true)))))
        .await;
    assert!(page(&harness).editor.is_none());
}

/// The read brings the connections' themes and the switch, on until stored off (RC-100-51).
#[tokio::test]
async fn the_read_brings_the_connections_themes_and_the_switch() {
    let db = Db::open_in_memory().await.expect("db");
    let draft = rc_engine::ConnectionDraft {
        theme_dark: Some("nord-dark".into()),
        ..rc_engine::ConnectionDraft::new("web", "example.org")
    };
    let web = db.create_connection(None, &draft).await.expect("web");
    let first = read(db.clone()).await;
    assert!(first.connections.on, "on by default");
    assert_eq!(first.connections.theme(web.id.0, true), Some("nord-dark"));
    assert_eq!(first.connections.theme(web.id.0, false), None);
    db.set(rc_engine::settings::CONNECTION_THEMES_KEY, &false)
        .await
        .expect("off");
    let off = read(db).await;
    assert!(!off.connections.on);
    assert_eq!(
        off.connections.theme(web.id.0, true),
        None,
        "off draws the window's"
    );
    assert_eq!(off.connections.themes.len(), 1, "the assignment stays");
}
