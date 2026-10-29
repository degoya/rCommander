use std::path::Path;

use rc_db::Db;
use rc_engine::{Command, PanelId, TabView};
use tempfile::TempDir;

use super::*;
use crate::app::harness::{self, Harness, headless_engine};
use crate::app::tabs::popup::PopupKey;

#[test]
fn a_press_becomes_a_drag_only_past_the_threshold() {
    let mut drag = TabDrag::press(PanelId::Left, TabId(1));
    drag.over(PanelId::Left, 2);
    assert_eq!(drag.gap, None, "no place before the drag starts");
    drag.moved(Point::new(10.0, 10.0));
    drag.moved(Point::new(13.0, 12.0));
    assert!(!drag.dragging, "a click that wobbles stays a click");
    drag.moved(Point::new(16.0, 10.0));
    assert!(drag.dragging);
    drag.over(PanelId::Right, 0);
    assert_eq!(drag.gap, None, "the other panel's bar is not this drag's");
    drag.over(PanelId::Left, 2);
    assert_eq!(drag.gap, Some(2));
}

#[test]
fn the_half_of_a_tab_says_before_or_after_it() {
    assert_eq!(gap_at(3, 10.0, 80.0), 3);
    assert_eq!(gap_at(3, 39.9, 80.0), 3);
    assert_eq!(gap_at(3, 40.0, 80.0), 4);
    assert_eq!(gap_at(0, 79.0, 80.0), 1);
}

#[test]
fn a_drop_beside_the_tab_itself_moves_nothing() {
    // Row [A, B, C]: A before C is place 1, A after C place 2; B before or after itself stays.
    assert_eq!(target(0, 2), Some(1));
    assert_eq!(target(0, 3), Some(2));
    assert_eq!(target(2, 0), Some(0));
    assert_eq!(target(1, 1), None);
    assert_eq!(target(1, 2), None);
    assert_eq!(target(0, 0), None);
}

/// A folder with `a` and `b`.
fn tree() -> TempDir {
    harness::tree(&[("a/", ""), ("b/", "")])
}

/// The left panel with three tabs - the folder, `a`, `b` (shown) - in a harness.
async fn three_tabs(root: &Path) -> Harness {
    let mut harness = Harness::with_db(root).await;
    for path in ["a", "../b"] {
        harness.press("Ctrl+T").await;
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
    assert_eq!(titles(&harness)[1..], ["a", "b"]);
    harness
}

fn strip(harness: &Harness) -> &[TabView] {
    &harness.app.tabs.strips[0]
}

fn titles(harness: &Harness) -> Vec<&str> {
    strip(harness)
        .iter()
        .map(|tab| tab.title.as_str())
        .collect()
}

fn shown(harness: &Harness) -> &str {
    strip(harness)
        .iter()
        .find(|tab| tab.active)
        .map_or("", |tab| tab.title.as_str())
}

async fn drag(harness: &mut Harness, message: DragMsg) {
    harness
        .send(Message::Tabs(TabsMessage::Drag(message)))
        .await;
}

/// Press on `tab`, then the pointer far enough for a drag.
async fn start_drag(harness: &mut Harness, tab: TabId) {
    drag(harness, DragMsg::Press(PanelId::Left, tab)).await;
    drag(harness, DragMsg::Moved(Point::new(20.0, 10.0))).await;
    drag(harness, DragMsg::Moved(Point::new(60.0, 10.0))).await;
}

/// The acceptance check of RC-010-24: the first tab dragged behind the last one goes there, the
/// shown tab stays shown, and the engine has the new order.
#[tokio::test]
async fn a_tab_dragged_to_the_end_of_its_bar_goes_there() {
    let root = tree();
    let mut harness = three_tabs(root.path()).await;
    let first = strip(&harness)[0].id;
    let name = titles(&harness)[0].to_owned();
    start_drag(&mut harness, first).await;
    assert!(harness.app.tabs_have_keys(), "Escape is the drag's");
    drag(&mut harness, DragMsg::Over(PanelId::Left, 3)).await;
    drag(&mut harness, DragMsg::Released).await;
    assert_eq!(titles(&harness), ["a", "b", name.as_str()]);
    assert_eq!(shown(&harness), "b");
    let engine = harness.app.engine().tabs(PanelId::Left);
    assert_eq!(engine[2].id, first);
    assert!(harness.app.tabs.drag.is_none());
}

/// A press released without a drag is a click: it shows the tab and moves nothing; so is a press
/// whose pointer stayed within the threshold.
#[tokio::test]
async fn a_press_without_a_drag_shows_the_tab() {
    let root = tree();
    let mut harness = three_tabs(root.path()).await;
    let first = strip(&harness)[0].id;
    let order = strip(&harness).to_vec();
    drag(&mut harness, DragMsg::Press(PanelId::Left, first)).await;
    drag(&mut harness, DragMsg::Released).await;
    assert_eq!(shown(&harness), titles(&harness)[0]);

    let second = strip(&harness)[1].id;
    drag(&mut harness, DragMsg::Press(PanelId::Left, second)).await;
    drag(&mut harness, DragMsg::Moved(Point::new(20.0, 10.0))).await;
    drag(&mut harness, DragMsg::Moved(Point::new(23.0, 11.0))).await;
    drag(&mut harness, DragMsg::Over(PanelId::Left, 3)).await;
    drag(&mut harness, DragMsg::Released).await;
    assert_eq!(shown(&harness), "a");
    let ids = |tabs: &[TabView]| tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
    assert_eq!(ids(strip(&harness)), ids(&order));
}

/// Escape, or the pointer leaving the bar, cancels a drag: the release then does nothing.
#[tokio::test]
async fn escape_or_leaving_the_bar_cancels_the_drag() {
    let root = tree();
    let mut harness = three_tabs(root.path()).await;
    let order = titles(&harness).join("|");
    let first = strip(&harness)[0].id;
    start_drag(&mut harness, first).await;
    drag(&mut harness, DragMsg::Over(PanelId::Left, 3)).await;
    harness.press("Escape").await;
    assert!(harness.app.tabs.drag.is_none());
    assert!(!harness.app.tabs_have_keys());
    drag(&mut harness, DragMsg::Released).await;
    assert_eq!(titles(&harness).join("|"), order);
    assert_eq!(shown(&harness), "b", "no click either");

    start_drag(&mut harness, first).await;
    drag(&mut harness, DragMsg::Over(PanelId::Left, 3)).await;
    drag(&mut harness, DragMsg::Cancel).await;
    drag(&mut harness, DragMsg::Released).await;
    assert_eq!(titles(&harness).join("|"), order);
}

/// A locked tab moves like any other (Total Commander), here to the front.
#[tokio::test]
async fn a_locked_tab_is_dragged_too() {
    let root = tree();
    let mut harness = three_tabs(root.path()).await;
    harness
        .app
        .engine()
        .dispatch(Command::LockTab(rc_engine::LockTabRequest::default()))
        .await
        .expect("lock");
    harness.settle().await;
    let last = strip(&harness)[2].id;
    start_drag(&mut harness, last).await;
    drag(&mut harness, DragMsg::Over(PanelId::Left, 0)).await;
    drag(&mut harness, DragMsg::Released).await;
    assert_eq!(strip(&harness)[0].id, last);
    assert!(strip(&harness)[0].locked && strip(&harness)[0].active);
}

/// With `tabs.drag_reorder` off a press shows its tab at once and starts no drag.
#[tokio::test]
async fn with_dragging_off_a_press_only_shows_the_tab() {
    let root = tree();
    let mut harness = three_tabs(root.path()).await;
    let settings = TabsMessage::Settings {
        drag: false,
        keys: true,
    };
    harness.send(Message::Tabs(settings)).await;
    let first = strip(&harness)[0].id;
    drag(&mut harness, DragMsg::Press(PanelId::Left, first)).await;
    assert!(!harness.app.tab_pressed());
    assert_eq!(shown(&harness), titles(&harness)[0]);
}

/// The tab's menu shows each entry's key from the keymap - Close tab Ctrl+W, the others none -
/// and none with `keys.context_menu_shortcuts` off.
#[tokio::test]
async fn the_tab_menu_shows_the_keymaps_keys() {
    let root = tree();
    let mut harness = three_tabs(root.path()).await;
    let first = strip(&harness)[0].id;
    let menu = Message::Tabs(TabsMessage::Menu(PanelId::Left, first));
    harness.send(menu.clone()).await;
    let keys = |harness: &Harness| -> Vec<Option<String>> {
        let popup = harness.app.tabs.popup.as_ref().expect("the menu");
        popup.rows.iter().map(|row| row.key.clone()).collect()
    };
    assert_eq!(keys(&harness), [None, None, Some("Ctrl+W".to_owned())]);

    let settings = TabsMessage::Settings {
        drag: true,
        keys: false,
    };
    harness.send(Message::Tabs(settings)).await;
    harness.send(menu).await;
    assert_eq!(keys(&harness), [None, None, None]);
    crate::app::context_menu::keys::set_shown(true);
}

/// Ctrl+Shift+A lists the tabs; Ctrl+Up there moves the tab under the cursor one place left,
/// the list staying open on it.
#[tokio::test]
async fn ctrl_up_in_the_list_of_tabs_moves_the_tab() {
    let root = tree();
    let mut harness = three_tabs(root.path()).await;
    harness.press("Ctrl+Shift+A").await;
    let popup = harness.app.tabs.popup.as_ref().expect("the list");
    assert_eq!(popup.cursor, 2, "on the shown tab");
    harness.press("Ctrl+ArrowUp").await;
    assert_eq!(titles(&harness)[1..], ["b", "a"]);
    let popup = harness.app.tabs.popup.as_ref().expect("still open");
    assert_eq!(popup.cursor, 1);
    assert_eq!(popup.rows[1].label, "b");
    assert!(matches!(
        popup.clone().key(&"Enter".parse().expect("chord")),
        PopupKey::Take(_)
    ));
}

/// Both settings are on unless stored off, and a change of either is read again.
#[tokio::test]
async fn the_settings_read_on_unless_stored_off() {
    let db = Db::open_in_memory().await.expect("db");
    assert_eq!(load(db.clone()).await, (true, true));
    let engine = headless_engine(&std::env::temp_dir()).await;
    assert!(engine.attach_db(db.clone()));
    engine
        .dispatch(Command::SetSetting {
            key: TAB_DRAG_KEY.to_owned(),
            value: false.into(),
        })
        .await
        .expect("set");
    assert_eq!(load(db).await, (false, true));

    let harness = Harness::new(engine);
    assert_eq!(harness.app.tabs_setting_changed(TAB_DRAG_KEY).len(), 1);
    assert_eq!(
        harness
            .app
            .tabs_setting_changed(CONTEXT_MENU_KEYS_KEY)
            .len(),
        1
    );
    assert!(
        harness
            .app
            .tabs_setting_changed("appearance.theme")
            .is_empty()
    );
}
