//! The bars through the app's `update()`: which bar shows where, a click and the keyboard run a
//! button, a sub-bar opens as a menu, the context menu deletes a button, sets the look and hides
//! the bar, a drop runs a program's button with the files or - with Shift - makes a new button.

use rc_engine::{
    BarButton, BarItem, BarPlace, BarSpec, ButtonAction, ButtonBar, Command, CommandKind,
    CommandRef, EngineEvent, ListMode, Outcome, PanelId,
};

use super::*;
use crate::{app::harness::Harness, i18n::t};

fn rows(row: &str) -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::Command(CommandRef {
            kind: CommandKind::Internal,
            command: row.into(),
            ..CommandRef::default()
        }),
        icon: None,
        tooltip: String::new(),
    })
}

fn sub_bar(id: i64) -> BarItem {
    BarItem::Button(BarButton {
        action: ButtonAction::SubBar(id),
        icon: None,
        tooltip: "More".into(),
    })
}

fn bar(id: i64, items: Vec<BarItem>) -> ButtonBar {
    ButtonBar {
        id,
        name: format!("Bar {id}"),
        spec: BarSpec {
            items,
            ..BarSpec::default()
        },
    }
}

/// A bar of `items` saved the window's way; its id.
async fn saved(harness: &Harness, name: &str, items: Vec<BarItem>) -> i64 {
    let outcome = harness
        .app
        .engine()
        .dispatch(Command::SaveButtonBar {
            id: None,
            name: name.into(),
            spec: BarSpec {
                items,
                ..BarSpec::default()
            },
        })
        .await
        .expect("saved");
    match outcome {
        Outcome::ButtonBars(outcome) => match *outcome {
            rc_engine::ButtonBarsOutcome::Saved { id } => id,
            other => panic!("saved, got {other:?}"),
        },
        other => panic!("a button-bars outcome, got {other:?}"),
    }
}

/// The engine's bars handed to the app, the bar `id` shown above the panels.
async fn shown(harness: &mut Harness, id: i64) {
    let bars = harness.app.engine().button_bars().await.expect("read");
    harness.send(wrap(BbMsg::Bars(Ok(bars)))).await;
    let settings = BarSettings {
        horizontal_bar: id,
        ..BarSettings::default()
    };
    harness.send(wrap(BbMsg::Settings(settings))).await;
}

#[test]
fn each_place_shows_the_bar_its_setting_names_or_the_first() {
    let mut ui = ButtonBarUi {
        bars: vec![bar(1, Vec::new()), bar(2, Vec::new())],
        ..ButtonBarUi::default()
    };
    assert_eq!(ui.shown(BarPlace::Horizontal).map(|bar| bar.id), Some(1));
    assert!(ui.shown(BarPlace::Vertical).is_none(), "off at the start");
    ui.settings.vertical = true;
    ui.settings.vertical_bar = 2;
    assert_eq!(ui.shown(BarPlace::Vertical).map(|bar| bar.id), Some(2));
    ui.settings.vertical_bar = 99;
    assert_eq!(
        ui.shown(BarPlace::Vertical).map(|bar| bar.id),
        Some(1),
        "a bar gone: the first"
    );
    ui.settings.horizontal = false;
    assert!(ui.shown(BarPlace::Horizontal).is_none());
    assert!(concerns("appearance.status_bar"));
    assert!(concerns("buttonbar.icon_size"));
    assert!(concerns("buttonbar.flat"));
    assert!(concerns("buttonbar.button_padding"));
    assert!(!concerns("appearance.theme"));
}

#[tokio::test]
async fn a_click_on_a_button_runs_its_command_for_the_active_panel() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    let id = saved(
        &harness,
        "Views",
        vec![BarItem::Separator, rows("view_brief")],
    )
    .await;
    shown(&mut harness, id).await;
    harness
        .send(wrap(BbMsg::Press(BarPlace::Horizontal, 1)))
        .await;
    assert_eq!(harness.app.pane(PanelId::Left).state.mode, ListMode::Brief);
    // A separator runs nothing.
    harness
        .send(wrap(BbMsg::Press(BarPlace::Horizontal, 0)))
        .await;
    assert!(harness.app.button_bar.menu.is_none());
}

#[tokio::test]
async fn the_keyboard_walks_a_bar_and_runs_a_button() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    let id = saved(
        &harness,
        "Keys",
        vec![rows("view_full"), BarItem::Separator, rows("view_brief")],
    )
    .await;
    shown(&mut harness, id).await;
    harness
        .send(Message::Engine(EngineEvent::ButtonBarFocus {
            bar: BarPlace::Horizontal,
        }))
        .await;
    let at = |harness: &Harness| harness.app.button_bar.focus.map(|focus| focus.index);
    assert_eq!(at(&harness), Some(0), "on the first button");
    harness.press("ArrowRight").await;
    assert_eq!(at(&harness), Some(2), "the separator passed over");
    harness.press("F5").await;
    assert_eq!(at(&harness), Some(2), "a panel's key stays in the bar");
    harness.press("Enter").await;
    assert_eq!(harness.app.pane(PanelId::Left).state.mode, ListMode::Brief);
    harness.press("Escape").await;
    assert_eq!(at(&harness), None, "back to the panel");
    // The vertical bar is off: its key does nothing.
    harness
        .send(Message::Engine(EngineEvent::ButtonBarFocus {
            bar: BarPlace::Vertical,
        }))
        .await;
    assert_eq!(at(&harness), None);
}

#[tokio::test]
async fn a_sub_bar_opens_as_a_menu_and_its_button_runs() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    let inner = saved(&harness, "Inner", vec![rows("view_brief")]).await;
    let outer = saved(&harness, "Outer", vec![sub_bar(inner)]).await;
    shown(&mut harness, outer).await;
    harness
        .send(wrap(BbMsg::Press(BarPlace::Horizontal, 0)))
        .await;
    let menu = harness.app.button_bar.menu.clone().expect("open");
    assert_eq!(
        menu.kind,
        MenuKind::Items {
            bar: inner,
            from: 0
        }
    );
    harness.send(wrap(BbMsg::Anchored(None))).await;
    harness.press("Enter").await;
    assert!(
        harness.app.button_bar.menu.is_none(),
        "closed after the run"
    );
    assert_eq!(harness.app.pane(PanelId::Left).state.mode, ListMode::Brief);

    harness
        .send(wrap(BbMsg::Press(BarPlace::Horizontal, 0)))
        .await;
    harness.press("Escape").await;
    assert!(harness.app.button_bar.menu.is_none(), "Escape closes it");
}

#[tokio::test]
async fn the_buttons_menu_deletes_it() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    let id = saved(&harness, "Two", vec![rows("view_full"), rows("view_brief")]).await;
    shown(&mut harness, id).await;
    harness
        .send(wrap(BbMsg::Context(BarPlace::Horizontal, Some(0))))
        .await;
    let menu = harness.app.button_bar.context.clone().expect("open");
    assert_eq!((menu.bar, menu.button), (id, Some(0)));
    harness.send(wrap(BbMsg::Ctx(CtxMsg::Pick(1)))).await;
    assert!(harness.app.button_bar.context.is_none(), "closed");
    let bars = harness.app.engine().button_bars().await.expect("read");
    let stored = bars.iter().find(|bar| bar.id == id).expect("there");
    assert_eq!(stored.spec.items, vec![rows("view_brief")]);
}

/// RC-110-15: beside the buttons the menu has the bar's entries only; Flat buttons and a size
/// are stored at once, Hide bar switches the bar off - also the vertical one, from the keyboard.
#[tokio::test]
async fn the_bars_menu_sets_the_look_and_hides_the_bar() {
    let root = tempfile::tempdir().expect("temp dir");
    // The engine's database, kept here: the window's own handle is set only by its start.
    let db = rc_db::Db::open_in_memory().await.expect("db");
    let mut harness = Harness::on_db(root.path(), db.clone()).await;
    let id = saved(
        &harness,
        "Look",
        vec![rows("view_full"), BarItem::Separator],
    )
    .await;
    shown(&mut harness, id).await;
    // A separator is beside the buttons.
    harness
        .send(wrap(BbMsg::Context(BarPlace::Horizontal, Some(1))))
        .await;
    let menu = harness.app.button_bar.context.clone().expect("open");
    assert_eq!(menu.button, None);
    let shown_items = menu.items(&harness.app.button_bar.settings);
    let flat = shown_items
        .iter()
        .position(|item| item.label == t!("button_bar.menu.flat"))
        .expect("Flat buttons");
    harness.send(wrap(BbMsg::Ctx(CtxMsg::Pick(flat)))).await;
    assert!(rc_engine::settings::stored_bool(&db, rc_engine::settings::FLAT_KEY).await);

    // Symbol size opens its sizes in the same card; 48 is stored as a number.
    harness
        .send(wrap(BbMsg::Context(BarPlace::Horizontal, None)))
        .await;
    harness.press("End").await;
    harness.press("ArrowUp").await;
    harness.press("ArrowUp").await;
    harness.press("ArrowRight").await;
    let menu = harness.app.button_bar.context.clone().expect("still open");
    assert_eq!(menu.level, context::Level::Sizes);
    harness.press("End").await;
    harness.press("ArrowUp").await;
    harness.press("Enter").await;
    assert!(harness.app.button_bar.context.is_none());
    assert_eq!(
        rc_engine::settings::stored_setting(&db, rc_engine::settings::ICON_SIZE_KEY).await,
        Some(serde_json::json!(48))
    );

    // The vertical bar from the keyboard: Shift+F10 on its button, the last entry hides it.
    let settings = BarSettings {
        vertical: true,
        vertical_bar: id,
        ..harness.app.button_bar.settings
    };
    harness.send(wrap(BbMsg::Settings(settings))).await;
    let vertical = rc_engine::BarKind::VerticalButtonBar.setting_key();
    harness
        .app
        .engine()
        .dispatch(Command::SetSetting {
            key: vertical.into(),
            value: serde_json::json!(true),
        })
        .await
        .expect("shown");
    harness
        .send(Message::Engine(EngineEvent::ButtonBarFocus {
            bar: BarPlace::Vertical,
        }))
        .await;
    harness.press("Shift+F10").await;
    let menu = harness.app.button_bar.context.clone().expect("open");
    assert_eq!((menu.place, menu.button), (BarPlace::Vertical, Some(0)));
    harness.press("End").await;
    harness.press("Enter").await;
    assert!(harness.app.button_bar.focus.is_none(), "the bar went");
    assert!(!rc_engine::settings::stored_bool(&db, vertical).await);
    // Escape closes an open menu, and a panel's key stays in it.
    harness
        .send(wrap(BbMsg::Context(BarPlace::Horizontal, None)))
        .await;
    harness.press("F5").await;
    assert!(harness.app.button_bar.context.is_some());
    harness.press("Escape").await;
    assert!(harness.app.button_bar.context.is_none());
}

#[tokio::test]
async fn a_drop_with_shift_makes_a_button_and_rows_take_no_files() {
    let (root, mut harness) = Harness::temp_with_db().await;
    let id = saved(&harness, "Drop", vec![rows("view_full")]).await;
    shown(&mut harness, id).await;
    let file = root.path().join("tool.exe");
    let target = Target {
        place: BarPlace::Horizontal,
        item: Some(0),
    };
    harness
        .send(wrap(BbMsg::Dropped {
            paths: vec![file.clone()],
            target: Some(target),
            shift: false,
            pointer: None,
        }))
        .await;
    assert!(harness.app.info.is_some(), "a row's button takes no files");
    harness
        .send(wrap(BbMsg::Dropped {
            paths: vec![file.clone()],
            target: Some(target),
            shift: true,
            pointer: None,
        }))
        .await;
    let bars = harness.app.engine().button_bars().await.expect("read");
    let stored = bars.iter().find(|bar| bar.id == id).expect("there");
    assert_eq!(stored.spec.items.len(), 2);
    let Some(BarItem::Button(made)) = stored.spec.items.get(1) else {
        panic!("a new button after the one it landed on");
    };
    let ButtonAction::Command(action) = &made.action else {
        panic!("a command");
    };
    assert_eq!(action.kind, CommandKind::Program);
    assert_eq!(action.command, file.to_string_lossy());
    assert_eq!(made.tooltip, "tool");
}

#[tokio::test]
async fn the_window_opens_on_the_bar_shown_and_closes_with_escape() {
    let (_root, mut harness) = Harness::temp_with_db().await;
    let id = saved(&harness, "Edit", vec![rows("view_full")]).await;
    shown(&mut harness, id).await;
    harness
        .send(Message::Engine(EngineEvent::ButtonBarWindow))
        .await;
    let window = harness.app.button_bar.window.as_ref().expect("open");
    assert_eq!(window.bar, Some(id));
    assert_eq!(window.name, "Edit");
    harness.send(wrap(BbMsg::Window(WinMsg::Close))).await;
    assert!(harness.app.button_bar.window.is_none());
}
