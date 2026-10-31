//! The focus ring (RC-080-16) of the settings window, page by page with their settings read,
//! the Host keys page with its table, and the imports: Total Commander's over the settings,
//! ssh_config's and Remote Desktop Manager's in the connection manager. Each walks round every
//! stop in reading order, Enter and Space press each button and switch, and no button is left
//! out of the ring.

use std::fs;

use iced::{Element, keyboard::key::Named};
use rc_db::Db;

use super::probe::{
    Screen, first_of, harness, host_key_question, kinds, press_at, press_every_stop, press_on,
    walk_round,
};
use crate::app::harness::headless_engine;
use crate::{
    app::{
        Message,
        connections::{self, ConnMsg, ImportMsg, RdmImportMsg, TcImportMsg},
        harness::Harness,
        hostkeys::{Choice, HostKeyMsg},
        settings::{self, Page, SettingsMessage, Tab},
    },
    theme::{self, focus::Kind, layout},
};

const TC_FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-engine/tests/fixtures/total_commander"
);
const RDM_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../rc-engine/tests/fixtures/rdm/sample.rdm"
);

/// The settings window's content, as its window draws it.
pub(super) fn settings_window(harness: &Harness) -> Element<'_, Message> {
    let tokens = theme::tokens(false);
    let dialog = harness.app.settings.as_ref().expect("the settings");
    layout::window_body(dialog.view(harness.app.settings_tabs, tokens), tokens)
}

/// The connection manager's layer: the tree and its form, or the import it shows.
fn manager_layer(harness: &Harness) -> Element<'_, Message> {
    let manager = harness.app.connections.as_ref().expect("the manager");
    connections::layer(manager, theme::tokens(false))
}

/// An engine and the app on one database, the settings open with what it holds.
pub(super) async fn settings_open() -> (tempfile::TempDir, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    let engine = headless_engine(root.path()).await;
    let db = Db::open_in_memory().await.expect("db");
    assert!(engine.attach_db(db.clone()));
    let mut harness = Harness::new(engine);
    harness.app.db = Some(db.clone());
    harness.send(Message::Settings(SettingsMessage::Open)).await;
    // What the dialog's task reads and hands over.
    let loaded = settings::load(harness.app.engine().clone(), db).await;
    harness
        .send(Message::Settings(SettingsMessage::Loaded(
            loaded.map(Box::new),
        )))
        .await;
    (root, harness)
}

#[tokio::test]
async fn every_settings_page_walks_its_controls() {
    let (_root, mut harness) = settings_open().await;
    for page in Page::ALL {
        harness
            .send(Message::Settings(SettingsMessage::Page(page)))
            .await;
        let name = format!("settings page {page:?}");
        let mut screen = Screen::new(settings_window(&harness)).await;
        let stops = walk_round(&mut screen, &name);
        assert_eq!(stops[0].kind, Some(Kind::Choice), "{name}: the pages first");
        assert!(stops.len() > 4, "{name}: the page's controls, {stops:?}");
        assert_eq!(screen.loose_buttons(), 0, "{name}: the title bar has the X");
        press_every_stop(&mut screen, &stops, &name);
        let cancel = press_on(&mut screen, "Cancel", Named::Enter);
        assert!(
            matches!(cancel[..], [Message::Settings(SettingsMessage::Cancel)]),
            "{name}"
        );
    }
}

/// A tab of the settings, as its strip shows it.
fn tab_title(tab: Tab) -> &'static str {
    match tab {
        Tab::Installed => "Installed",
        Tab::PluginSettings => "Global settings",
        Tab::Window => "Theme and window",
        Tab::Fonts => "Fonts",
        Tab::Lists => "Panels and lists",
        Tab::Viewer => "Viewer",
        Tab::Themes => "Themes",
        Tab::Keys => "General",
        Tab::Shortcuts => "Shortcuts",
    }
}

fn is_tab(messages: &[Message], tab: Tab) -> bool {
    matches!(messages, [Message::Settings(SettingsMessage::Tab(chosen))] if *chosen == tab)
}

/// The Plugins, Appearance and Keyboard pages show their tabs as one stop after the page list (RC-100-20):
/// the arrows choose the tab beside the chosen one, a click the tab clicked, and every tab walks
/// its controls.
#[tokio::test]
async fn the_tabs_of_a_page_are_one_stop_and_take_arrows_and_clicks() {
    let (_root, mut harness) = settings_open().await;
    let pages = [
        (Page::Plugins, vec![Tab::Installed, Tab::PluginSettings]),
        (
            Page::Appearance,
            vec![
                Tab::Window,
                Tab::Fonts,
                Tab::Themes,
                Tab::Lists,
                Tab::Viewer,
            ],
        ),
        (Page::Keyboard, vec![Tab::Keys, Tab::Shortcuts]),
    ];
    for (page, tabs) in pages {
        harness
            .send(Message::Settings(SettingsMessage::Page(page)))
            .await;
        for (at, tab) in tabs.iter().copied().enumerate() {
            harness
                .send(Message::Settings(SettingsMessage::Tab(tab)))
                .await;
            let name = format!("settings {page:?}, tab {tab:?}");
            let mut screen = Screen::new(settings_window(&harness)).await;
            let stops = walk_round(&mut screen, &name);
            assert_eq!(
                kinds(&stops)[..2],
                [Some(Kind::Choice), Some(Kind::Choice)],
                "{name}: the pages, then the tabs"
            );
            assert!(stops[1].text.contains(tab_title(tab)), "{name}: {stops:?}");
            assert_eq!(screen.loose_buttons(), 0, "{name}");
            press_every_stop(&mut screen, &stops, &name);
            let (key, beside) = match at.checked_sub(1) {
                Some(before) => (Named::ArrowLeft, tabs[before]),
                None => (Named::ArrowRight, tabs[1]),
            };
            let arrow = press_at(&mut screen, 1, key);
            assert!(is_tab(&arrow, beside), "{name}: {key:?} gave {arrow:?}");
            let next = tabs[(at + 1) % tabs.len()];
            let clicked = screen.click(tab_title(next));
            assert!(is_tab(&clicked, next), "{name}: a click gave {clicked:?}");
        }
    }
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Plugins)))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Tab(Tab::Installed)))
        .await;
    let mut screen = Screen::new(settings_window(&harness)).await;
    let stops = screen.stops();
    assert!(
        !stops
            .iter()
            .any(|seen| seen.text.contains("Development mode")),
        "the schema rows are on the other tab: {stops:?}"
    );
}

/// The table of trusted keys is a stop: on it the arrows are the page's and select a key, and
/// Remove joins the ring.
#[tokio::test]
async fn the_host_keys_page_has_its_table_in_the_ring() {
    let (_root, mut harness) = settings_open().await;
    host_key_question(&mut harness).await;
    let id = harness
        .app
        .host_keys
        .shown()
        .map(|shown| shown.question.id)
        .expect("the question");
    harness
        .send(Message::HostKey(HostKeyMsg::Choose(
            id,
            Choice::TrustAndSave,
        )))
        .await;
    assert!(harness.app.host_keys.shown().is_none(), "trusted");
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::HostKeys)))
        .await;
    let count = {
        let mut screen = Screen::new(settings_window(&harness)).await;
        let stops = walk_round(&mut screen, "host keys");
        let table = stops
            .iter()
            .enumerate()
            .filter(|(_, seen)| seen.kind == Some(Kind::Choice))
            .nth(1)
            .map(|(at, _)| at)
            .expect("the table, after the pages");
        assert_eq!(stops[table - 1].kind, None, "after the search");
        assert_eq!(screen.loose_buttons(), 0);
        for _ in 0..stops.len() {
            if screen.at() == Some(table) {
                break;
            }
            screen.tab(false);
        }
        assert_eq!(screen.at(), Some(table));
        let (messages, taken) = screen.press(Named::ArrowDown);
        assert!(messages.is_empty() && !taken, "the arrow is the page's");
        stops.len()
    };
    harness.press("ArrowDown").await;
    let mut screen = Screen::new(settings_window(&harness)).await;
    let stops = screen.stops();
    assert_eq!(stops.len(), count + 1, "Remove, with a key selected");
    let remove = stops
        .iter()
        .rposition(|seen| seen.kind == Some(Kind::Button) && seen.text.contains("Remove"))
        .expect("Remove");
    let removing = press_at(&mut screen, remove, Named::Enter);
    assert!(matches!(
        removing[..],
        [Message::Settings(SettingsMessage::HostKeys(_))]
    ));
}

/// The settings' Import page opens Total Commander's import over them: the files, the rows'
/// boxes, the two choices, Back and Close; then the master-password prompt the passwords ask
/// for.
#[tokio::test]
async fn the_total_commander_import_walks_its_files_rows_and_buttons() {
    let (root, mut harness) = settings_open().await;
    let wincmd = root.path().join("wincmd.ini");
    fs::copy(format!("{TC_FIXTURES}/wincmd_utf16.ini"), &wincmd).expect("wincmd");
    fs::copy(
        format!("{TC_FIXTURES}/wcx_ftp.ini"),
        root.path().join("wcx_ftp.ini"),
    )
    .expect("wcx_ftp");
    let tc = |message| Message::Connections(ConnMsg::TcImport(message));
    harness.send(tc(TcImportMsg::Open)).await;
    harness
        .send(tc(TcImportMsg::Wincmd(wincmd.display().to_string())))
        .await;
    harness.send(tc(TcImportMsg::Read)).await;
    {
        let mut screen = Screen::new(manager_layer(&harness)).await;
        let stops = walk_round(&mut screen, "Total Commander import");
        assert_eq!(stops[0].kind, None, "wincmd.ini first");
        assert!(
            kinds(&stops).contains(&Some(Kind::Toggle)),
            "the rows' boxes"
        );
        assert_eq!(screen.loose_buttons(), 1, "only the head's X");
        press_every_stop(&mut screen, &stops, "Total Commander import");
        let passwords = press_on(&mut screen, "Take the passwords", Named::Space);
        assert!(matches!(
            passwords[..],
            [Message::Connections(ConnMsg::TcImport(
                TcImportMsg::Passwords(true)
            ))]
        ));
        let back = press_on(&mut screen, "Back", Named::Enter);
        assert!(matches!(
            back[..],
            [Message::Connections(ConnMsg::TcImport(TcImportMsg::Back))]
        ));
    }
    // Asked for, the passwords wait for a master password: its prompt in the import's place.
    harness.send(tc(TcImportMsg::Passwords(true))).await;
    harness.send(tc(TcImportMsg::Run)).await;
    let mut screen = Screen::new(manager_layer(&harness)).await;
    let stops = walk_round(&mut screen, "master password");
    assert_eq!(kinds(&stops)[..2], [None, None], "the password, once more");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "master password");
    let cancel = press_on(&mut screen, "Cancel", Named::Enter);
    assert!(matches!(
        cancel[..],
        [Message::Connections(ConnMsg::VaultCancel)]
    ));
}

/// The manager's ssh_config import opens on a preview of the person's own config, so its rows
/// differ from one machine to the next: the source, the target and the buttons around them.
#[tokio::test]
async fn the_ssh_config_import_walks_its_source_target_and_buttons() {
    let (_root, mut harness) = harness().await;
    harness.press("Ctrl+F").await;
    harness
        .send(Message::Connections(ConnMsg::Import(ImportMsg::Open)))
        .await;
    let mut screen = Screen::new(manager_layer(&harness)).await;
    let stops = walk_round(&mut screen, "ssh_config import");
    assert_eq!(stops[0].kind, None, "the config's path first");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    assert!(
        kinds(&stops).contains(&Some(Kind::Choice)),
        "the target folder"
    );
    press_every_stop(&mut screen, &stops, "ssh_config import");
    let close = press_at(&mut screen, stops.len() - 1, Named::Enter);
    assert!(matches!(close[..], [Message::Connections(ConnMsg::Close)]));
}

#[tokio::test]
async fn the_rdm_import_walks_its_file_rows_and_buttons() {
    let (root, mut harness) = harness().await;
    let export = root.path().join("export.rdm");
    fs::copy(RDM_FIXTURE, &export).expect("fixture");
    harness.press("Ctrl+F").await;
    let rdm = |message| Message::Connections(ConnMsg::RdmImport(message));
    harness.send(rdm(RdmImportMsg::Open)).await;
    harness
        .send(rdm(RdmImportMsg::Path(export.display().to_string())))
        .await;
    harness.send(rdm(RdmImportMsg::Read)).await;
    let mut screen = Screen::new(manager_layer(&harness)).await;
    let stops = walk_round(&mut screen, "RDM import");
    assert_eq!(stops[0].kind, None, "the export's path first");
    assert_eq!(screen.loose_buttons(), 1, "only the head's X");
    press_every_stop(&mut screen, &stops, "RDM import");
    let row = press_at(&mut screen, first_of(&stops, Kind::Toggle), Named::Space);
    assert!(matches!(
        row[..],
        [Message::Connections(ConnMsg::RdmImport(
            RdmImportMsg::Choose(_, _)
        ))]
    ));
    let back = press_on(&mut screen, "Back", Named::Enter);
    assert!(matches!(
        back[..],
        [Message::Connections(ConnMsg::RdmImport(RdmImportMsg::Back))]
    ));
}

/// The Shortcuts tab (RC-100-21): the search, the table of keys as one stop whose arrows are the
/// tab's, then the row's buttons; with a row selected they join the ring, and the question
/// before a key moves has its two buttons in it.
#[tokio::test]
async fn the_shortcuts_tab_has_its_table_and_buttons_in_the_ring() {
    let (_root, mut harness) = settings_open().await;
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Keyboard)))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Tab(Tab::Shortcuts)))
        .await;
    let count = {
        let mut screen = Screen::new(settings_window(&harness)).await;
        let stops = walk_round(&mut screen, "shortcuts");
        let table = stops
            .iter()
            .enumerate()
            .filter(|(_, seen)| seen.kind == Some(Kind::Choice))
            .nth(2)
            .map(|(at, _)| at)
            .expect("the table, after the pages and the tabs");
        assert_eq!(stops[table - 1].kind, None, "after the search");
        assert_eq!(screen.loose_buttons(), 0);
        press_every_stop(&mut screen, &stops, "shortcuts");
        stops.len()
    };
    harness.press("ArrowDown").await;
    let mut screen = Screen::new(settings_window(&harness)).await;
    let stops = walk_round(&mut screen, "shortcuts, a row selected");
    assert_eq!(
        stops.len(),
        count + 4,
        "Change key, Add key, Remove key and Reset: {stops:?}"
    );
    assert_eq!(screen.loose_buttons(), 0);
    let change = press_on(&mut screen, "Change key", Named::Enter);
    assert!(
        matches!(
            change[..],
            [Message::Settings(SettingsMessage::Shortcuts(_))]
        ),
        "{change:?}"
    );
    drop(screen);
    for message in change {
        harness.send(message).await;
    }
    harness.press("F6").await;
    let mut screen = Screen::new(settings_window(&harness)).await;
    let stops = walk_round(&mut screen, "shortcuts, the question");
    for label in ["Replace", "Keep"] {
        assert!(
            stops.iter().any(|seen| seen.text.contains(label)),
            "{label}: {stops:?}"
        );
    }
    assert_eq!(screen.loose_buttons(), 0);
}

/// The Themes tab and its editor (RC-100-50): every control is a stop of the ring, no button is
/// left out - the modes' pick lists, the list, the buttons, the file, the rules; in the editor the
/// name, the accent's switch and every colour's field.
#[tokio::test]
async fn the_themes_tab_and_its_editor_walk_their_controls() {
    use rc_engine::{Command, Outcome, settings::themes::ThemeDraft};
    use settings::ThemesMsg;

    let themes = |message| Message::Settings(SettingsMessage::Themes(message));
    let (_root, mut harness) = settings_open().await;
    let db = harness.app.db.clone().expect("the database");
    let read = settings::read_themes(db.clone()).await;
    harness.send(themes(ThemesMsg::Read(Box::new(read)))).await;
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Appearance)))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Tab(Tab::Themes)))
        .await;
    {
        let mut screen = Screen::new(settings_window(&harness)).await;
        let stops = walk_round(&mut screen, "the Themes tab");
        assert!(stops.len() > 8, "the Themes tab: {stops:?}");
        assert_eq!(screen.loose_buttons(), 0, "the Themes tab");
    }
    let saved = harness
        .app
        .engine()
        .dispatch(Command::SaveTheme {
            theme: ThemeDraft {
                id: None,
                name: "Ring".into(),
                dark: false,
                accent: "system".into(),
                tokens: std::collections::BTreeMap::new(),
            },
        })
        .await
        .expect("saved");
    let Outcome::Themes(outcome) = saved else {
        panic!("the themes");
    };
    let read = settings::read_themes(db).await;
    harness.send(themes(ThemesMsg::Read(Box::new(read)))).await;
    let id = outcome.saved.expect("its id");
    harness.send(themes(ThemesMsg::Choose(id))).await;
    harness.send(themes(ThemesMsg::Edit)).await;
    let mut screen = Screen::new(settings_window(&harness)).await;
    let stops = walk_round(&mut screen, "the theme editor");
    assert!(stops.len() > 45, "every colour's field: {}", stops.len());
    assert_eq!(screen.loose_buttons(), 0, "the theme editor");
}
