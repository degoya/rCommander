//! The context menu of a theme's row (RC-110-09): its keys and place without a window, then its
//! entries over the app's `update()` with the dialog's draft.

use std::collections::BTreeMap;

use iced::{Point, Rectangle, Vector, advanced::widget::Id};
use rc_db::Db;
use rc_engine::{
    Chord, Command,
    settings::{THEME_DARK_KEY, THEME_LIGHT_KEY, themes::ThemeDraft},
};
use tempfile::TempDir;

use super::{Entry, Item, LIST_STOP, MenuKey, MenuMsg, ThemeMenu, is_menu_key, spot};
use crate::app::{
    Message,
    context_menu::MENU_WIDTH,
    harness::Harness,
    settings::{
        self, Page, SettingsMessage, Tab,
        draft::Field,
        themes_page::{ThemesMsg, actions, read, usage::Environment},
    },
};

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

fn item(label: &str, enabled: bool) -> Item {
    Item {
        entry: Entry::Duplicate,
        label: label.to_owned(),
        checked: false,
        enabled,
    }
}

/// Use, Edit (greyed out), Duplicate, Rename (greyed out), Delete.
fn menu() -> ThemeMenu {
    ThemeMenu::new(
        "nord-dark".into(),
        vec![
            item("Use for dark mode", true),
            item("Edit...", false),
            item("Duplicate", true),
            item("Rename", false),
            item("Delete", true),
        ],
    )
}

#[test]
fn shift_f10_and_the_menu_key_open_it_nothing_else() {
    assert!(is_menu_key(&chord("Shift+F10")));
    assert!(is_menu_key(&chord("ContextMenu")));
    assert!(!is_menu_key(&chord("F10")));
    assert!(!is_menu_key(&chord("Ctrl+Shift+F10")));
    assert!(!is_menu_key(&chord("Shift+ContextMenu")));
}

#[test]
fn the_keys_move_past_greyed_out_entries_and_round_the_ends() {
    let mut open = menu();
    assert_eq!(open.cursor, 0);
    assert_eq!(open.key(&chord("ArrowDown")), MenuKey::Stay);
    assert_eq!(open.cursor, 2, "Edit... is greyed out");
    open.key(&chord("ArrowDown"));
    assert_eq!(open.cursor, 4, "Rename is greyed out");
    open.key(&chord("ArrowDown"));
    assert_eq!(open.cursor, 0, "round the end");
    open.key(&chord("ArrowUp"));
    assert_eq!(open.cursor, 4, "round the start");
    open.key(&chord("Home"));
    assert_eq!(open.cursor, 0);
    open.key(&chord("End"));
    assert_eq!(open.cursor, 4);
    assert_eq!(open.key(&chord("Enter")), MenuKey::Run(4));
    assert_eq!(open.key(&chord("Space")), MenuKey::Run(4));
    assert_eq!(open.key(&chord("F5")), MenuKey::Stay, "swallowed");
    assert_eq!(open.key(&chord("Escape")), MenuKey::Close);
    assert_eq!(open.key(&chord("Shift+F10")), MenuKey::Close);
    assert_eq!(open.key(&chord("ContextMenu")), MenuKey::Close);
}

#[test]
fn a_letter_takes_its_one_entry_and_passes_greyed_out_ones() {
    let mut open = menu();
    assert_eq!(open.key(&chord("x")), MenuKey::Stay, "no entry");
    assert_eq!(open.key(&chord("r")), MenuKey::Stay, "Rename is greyed out");
    assert_eq!(open.key(&chord("u")), MenuKey::Run(0));
    assert_eq!(open.key(&chord("d")), MenuKey::Stay, "Duplicate and Delete");
    assert_eq!(open.cursor, 2);
    open.key(&chord("d"));
    assert_eq!(open.cursor, 4);
}

#[test]
fn a_menu_without_an_entry_that_runs_closes_on_enter() {
    let mut open = ThemeMenu::new("x".into(), vec![item("Edit...", false)]);
    assert_eq!(open.cursor, 0);
    assert_eq!(open.key(&chord("ArrowDown")), MenuKey::Stay);
    assert_eq!(open.cursor, 0);
    assert_eq!(open.key(&chord("Enter")), MenuKey::Close);
}

#[test]
fn the_menu_stands_under_its_row_or_over_it_inside_the_area() {
    let area = Rectangle::new(Point::new(200.0, 100.0), iced::Size::new(600.0, 400.0));
    let row = Rectangle::new(Point::new(212.0, 150.0), iced::Size::new(560.0, 26.0));
    let at = spot(area, Vector::ZERO, row, 5);
    assert_eq!(at, Point::new(36.0, 76.0), "under the row, indented");
    let scrolled = spot(area, Vector::new(0.0, 40.0), row, 5);
    assert_eq!(scrolled.y, 36.0, "the row scrolled up");
    let low = Rectangle::new(Point::new(212.0, 450.0), iced::Size::new(560.0, 26.0));
    let card = 5.0 * crate::theme::ROW_HEIGHT + 10.0;
    let over = spot(area, Vector::ZERO, low, 5);
    assert_eq!(over.y, 450.0 - card - 100.0, "over the row near the bottom");
    let gone = spot(area, Vector::new(0.0, 300.0), row, 5);
    assert_eq!(gone.y, 0.0, "a row scrolled out of sight: the area's top");
    let narrow = Rectangle::new(Point::new(200.0, 100.0), iced::Size::new(300.0, 400.0));
    let at = spot(narrow, Vector::ZERO, row, 5);
    assert_eq!(at.x, 300.0 - MENU_WIDTH, "kept inside on the right");
}

fn themes(message: ThemesMsg) -> Message {
    Message::Settings(SettingsMessage::Themes(message))
}

fn menu_message(message: MenuMsg) -> Message {
    themes(ThemesMsg::Menu(message))
}

/// The dialog open on the Themes tab with its draft, the themes read.
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
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Appearance)))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Tab(Tab::Themes)))
        .await;
    (root, harness)
}

async fn reread(harness: &mut Harness, db: &Db) {
    let read = read(db.clone()).await;
    harness.send(themes(ThemesMsg::Read(Box::new(read)))).await;
}

fn page(harness: &Harness) -> &super::super::ThemesPage {
    &harness.app.settings.as_ref().expect("the dialog").themes
}

fn open_menu(harness: &Harness) -> &ThemeMenu {
    page(harness).menu.as_ref().expect("the menu")
}

fn drafted(harness: &Harness, key: &str) -> Option<String> {
    let dialog = harness.app.settings.as_ref().expect("the dialog");
    match dialog.draft.as_ref().and_then(|draft| draft.field(key)) {
        Some(Field::Text(text)) => Some(text.clone()),
        _ => None,
    }
}

/// A right click marks the theme and opens its menu; the place measured shows it; "Use for dark
/// mode" edits the draft as the pick list does, and the entry is checked then.
#[tokio::test]
async fn a_right_click_uses_a_theme_for_its_mode() {
    let db = Db::open_in_memory().await.expect("db");
    let (_root, mut harness) = open(&db).await;
    harness
        .send(menu_message(MenuMsg::Open("nord-dark".into())))
        .await;
    assert_eq!(page(&harness).selected.as_deref(), Some("nord-dark"));
    let menu = open_menu(&harness);
    assert_eq!(menu.theme, "nord-dark");
    assert_eq!(menu.at, None, "not shown before it is measured");
    assert_eq!(
        menu.items[0].entry,
        Entry::Use(Environment::Mode { dark: true })
    );
    assert!(!menu.items[0].checked);
    let actions: Vec<(&Entry, bool)> = menu.items[1..]
        .iter()
        .map(|item| (&item.entry, item.enabled))
        .collect();
    assert_eq!(
        actions,
        vec![
            (&Entry::Edit, false),
            (&Entry::Duplicate, true),
            (&Entry::Rename, false),
            (&Entry::Delete, false),
        ],
        "a built-in theme is only duplicated"
    );
    harness
        .send(menu_message(MenuMsg::Placed(Some(Point::new(24.0, 80.0)))))
        .await;
    assert_eq!(open_menu(&harness).at, Some(Point::new(24.0, 80.0)));
    harness.send(menu_message(MenuMsg::Pick(0))).await;
    assert!(page(&harness).menu.is_none(), "the entry closes the menu");
    assert_eq!(
        drafted(&harness, THEME_DARK_KEY).as_deref(),
        Some("nord-dark")
    );
    harness
        .send(menu_message(MenuMsg::Open("nord-dark".into())))
        .await;
    assert!(open_menu(&harness).items[0].checked, "used now");
    harness.send(menu_message(MenuMsg::Pick(0))).await;
    assert_eq!(
        drafted(&harness, THEME_DARK_KEY).as_deref(),
        Some("nord-dark"),
        "a checked entry changes nothing"
    );
    harness
        .send(menu_message(MenuMsg::Open("nord-light".into())))
        .await;
    assert_eq!(
        open_menu(&harness).items[0].entry,
        Entry::Use(Environment::Mode { dark: false }),
        "a light theme is offered for the light mode"
    );
    harness.send(menu_message(MenuMsg::Close)).await;
    assert!(page(&harness).menu.is_none());
    assert_ne!(
        drafted(&harness, THEME_LIGHT_KEY).as_deref(),
        Some("nord-light")
    );
}

/// Shift+F10 asks which stop has the keyboard: the list or none opens the marked theme's menu,
/// another stop does not. While it is open the keys are the menu's: Escape closes it, not the
/// dialog; Enter runs the entry, not OK.
#[tokio::test]
async fn the_keyboard_opens_works_and_closes_the_menu() {
    let db = Db::open_in_memory().await.expect("db");
    let (_root, mut harness) = open(&db).await;
    harness
        .send(themes(ThemesMsg::Choose("nord-dark".into())))
        .await;
    harness.press("Shift+F10").await;
    assert!(
        page(&harness).menu.is_none(),
        "the menu waits for the stop that has the keyboard"
    );
    harness
        .send(menu_message(MenuMsg::Wanted(Some(Id::new(
            "settings.pages",
        )))))
        .await;
    assert!(page(&harness).menu.is_none(), "another stop has it");
    harness
        .send(menu_message(MenuMsg::Wanted(Some(Id::new(LIST_STOP)))))
        .await;
    assert_eq!(open_menu(&harness).theme, "nord-dark");
    harness.press("Escape").await;
    assert!(page(&harness).menu.is_none(), "Escape closes the menu");
    assert!(harness.app.settings.is_some(), "and leaves the dialog open");
    harness.send(menu_message(MenuMsg::Wanted(None))).await;
    assert!(page(&harness).menu.is_some(), "no stop has the keyboard");
    harness.press("ArrowDown").await;
    assert_eq!(open_menu(&harness).cursor, 2, "past Edit..., greyed out");
    harness.press("Enter").await;
    assert!(harness.app.settings.is_some(), "Enter is the menu's");
    assert!(page(&harness).menu.is_none());
    assert!(page(&harness).busy, "Duplicate runs");
}

/// An own theme's menu presses the row's buttons: Rename asks for the name; Delete is greyed out
/// while a mode draws with it.
#[tokio::test]
async fn an_own_theme_s_menu_presses_the_row_s_buttons() {
    let db = Db::open_in_memory().await.expect("db");
    let (_root, mut harness) = open(&db).await;
    let outcome = actions::run(
        harness.app.engine().clone(),
        Command::SaveTheme {
            theme: ThemeDraft {
                id: None,
                name: "Night".into(),
                dark: true,
                accent: "system".into(),
                tokens: BTreeMap::new(),
            },
        },
    )
    .await
    .expect("saved");
    let id = outcome.saved.expect("its id");
    reread(&mut harness, &db).await;
    harness.send(menu_message(MenuMsg::Open(id.clone()))).await;
    let enabled: Vec<bool> = open_menu(&harness)
        .items
        .iter()
        .map(|item| item.enabled)
        .collect();
    assert_eq!(enabled, vec![true, true, true, true, true]);
    let rename = open_menu(&harness)
        .items
        .iter()
        .position(|item| item.entry == Entry::Rename)
        .expect("Rename");
    harness.send(menu_message(MenuMsg::Pick(rename))).await;
    assert_eq!(page(&harness).renaming.as_deref(), Some("Night"));
    assert_eq!(page(&harness).selected.as_deref(), Some(id.as_str()));
    harness.send(themes(ThemesMsg::RenameCancel)).await;
    harness
        .app
        .engine()
        .dispatch(Command::SetSetting {
            key: THEME_DARK_KEY.into(),
            value: serde_json::json!(id.clone()),
        })
        .await
        .expect("set");
    reread(&mut harness, &db).await;
    harness.send(menu_message(MenuMsg::Open(id.clone()))).await;
    let delete = open_menu(&harness)
        .items
        .iter()
        .find(|item| item.entry == Entry::Delete)
        .expect("Delete");
    assert!(!delete.enabled, "the dark mode draws with it");
}

/// Turning to another page closes the menu; Shift+F10 on another page is not the tab's.
#[tokio::test]
async fn the_menu_closes_with_its_tab() {
    let db = Db::open_in_memory().await.expect("db");
    let (_root, mut harness) = open(&db).await;
    harness
        .send(menu_message(MenuMsg::Open("nord-dark".into())))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Tab(Tab::Fonts)))
        .await;
    assert!(page(&harness).menu.is_none());
    harness
        .send(menu_message(MenuMsg::Open("nord-dark".into())))
        .await;
    harness
        .send(Message::Settings(SettingsMessage::Page(Page::Keyboard)))
        .await;
    assert!(page(&harness).menu.is_none());
}
