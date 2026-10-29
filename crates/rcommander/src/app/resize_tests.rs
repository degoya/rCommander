use iced::{Point, Size};
use rc_db::Db;
use rc_engine::settings::{
    DATE_WIDTH_KEY, EXT_WIDTH_KEY, OWNER_COLUMN_KEY, PANEL_SPLIT_KEY, THEME_KEY,
};
use serde_json::Value;

use super::*;
use crate::app::{harness::Harness, palette::PaletteMsg};

async fn harness() -> (tempfile::TempDir, Db, Harness) {
    let root = tempfile::tempdir().expect("temp dir");
    let db = Db::open_in_memory().await.expect("db");
    let harness = Harness::on_db(root.path(), db.clone()).await;
    (root, db, harness)
}

fn moved(x: f32) -> Message {
    wrap(ResizeMsg::Moved(Point::new(x, 10.0)))
}

#[test]
fn the_divider_holds_its_bounds_and_each_panel_its_least_width() {
    // Not measured yet: the schema's bounds.
    assert_eq!(clamp_split(500.0, 0.0), 500);
    assert_eq!(clamp_split(20.0, 0.0), 100);
    assert_eq!(clamp_split(990.0, 0.0), 900);
    assert_eq!(clamp_split(f32::NAN, 0.0), 500, "no number is the middle");
    // 1008 wide: 1000 for the panels, each keeps 240 of them.
    assert_eq!(clamp_split(612.4, 1008.0), 612);
    assert_eq!(clamp_split(100.0, 1008.0), 240);
    assert_eq!(clamp_split(900.0, 1008.0), 760);
    // Too narrow for two panels of 240: the middle.
    assert_eq!(clamp_split(300.0, 400.0), 500);
}

#[test]
fn the_widths_and_the_divider_are_the_settings_it_follows() {
    for column in Column::FIXED {
        let key = column.setting().expect("a fixed column has its setting");
        assert!(concerns(key), "{key}");
    }
    assert!(concerns(PANEL_SPLIT_KEY));
    assert!(concerns(OWNER_COLUMN_KEY));
    assert!(!concerns(THEME_KEY));
}

/// RC-060-02: the owner column is off until the person switches it on.
#[tokio::test]
async fn the_owner_column_is_read_with_the_widths() {
    let (_root, db, _harness) = harness().await;
    assert!(!load(db.clone()).await.0.owner, "off by default");
    db.set(OWNER_COLUMN_KEY, &true).await.expect("stored");
    assert!(load(db).await.0.owner);
}

/// A drag at a column's grip changes its width as the pointer goes; the release stores it, a
/// double-click the default. A click that did not move stores nothing.
#[tokio::test]
async fn a_dragged_column_is_stored_and_a_double_click_restores_its_default() {
    let (_root, db, mut harness) = harness().await;
    let date = Column::Date.bounds().2;
    let grab = wrap(ResizeMsg::Grab(Target::Column(Column::Date)));
    harness.send(grab.clone()).await;
    assert!(harness.app.resize.grabbing());
    harness.send(wrap(ResizeMsg::Released)).await;
    assert!(!harness.app.resize.grabbing());
    let stored = db.get::<Value>(DATE_WIDTH_KEY).await.expect("reads");
    assert_eq!(stored, None, "a click without a move stores nothing");

    let scale = fonts::list().column_scale();
    harness.send(grab).await;
    harness.send(moved(500.0)).await;
    harness.send(moved(460.0)).await;
    let wider = date + 40.0 / scale;
    assert_eq!(
        harness.app.resize.columns.get(Column::Date),
        Some(wider),
        "its left edge went 40 to the left"
    );
    harness.send(wrap(ResizeMsg::Released)).await;
    let (columns, split) = load(db.clone()).await;
    assert_eq!(columns.get(Column::Date), Some(wider.round()));
    assert_eq!(
        columns.get(Column::Ext),
        Some(Column::Ext.bounds().2),
        "the others keep theirs"
    );
    assert_eq!(split, 500.0);

    harness
        .send(wrap(ResizeMsg::Reset(Target::Column(Column::Date))))
        .await;
    assert!(!harness.app.resize.grabbing());
    let (columns, _) = load(db.clone()).await;
    assert_eq!(columns.get(Column::Date), Some(date));

    // Dragged far to the right: the least width, as stored.
    harness
        .send(wrap(ResizeMsg::Grab(Target::Column(Column::Ext))))
        .await;
    harness.send(moved(0.0)).await;
    harness.send(moved(900.0)).await;
    harness.send(wrap(ResizeMsg::Released)).await;
    let stored = db.get::<Value>(EXT_WIDTH_KEY).await.expect("reads");
    assert_eq!(stored, Some(Value::from(24)));
}

/// The divider follows the pointer within its bounds and is stored on release; the command
/// palette's "Divider to 50 %" puts it back to the middle, stored too.
#[tokio::test]
async fn the_divider_is_dragged_stored_and_centred_from_the_palette() {
    let (_root, db, mut harness) = harness().await;
    harness
        .send(wrap(ResizeMsg::Area(Size::new(1008.0, 600.0))))
        .await;
    harness.send(wrap(ResizeMsg::Grab(Target::Divider))).await;
    harness.send(moved(500.0)).await;
    harness.send(moved(600.0)).await;
    assert_eq!(harness.app.resize.left_share(), 600);
    harness.send(moved(2000.0)).await;
    assert_eq!(
        harness.app.resize.left_share(),
        760,
        "the right panel keeps 240"
    );
    harness.send(wrap(ResizeMsg::Released)).await;
    assert_eq!(load(db.clone()).await.1, 760.0);

    harness.press("Ctrl+Shift+P").await;
    let palette = harness.app.palette.as_ref().expect("open");
    let at = palette
        .shown
        .iter()
        .position(|index| {
            palette
                .entry(*index)
                .is_some_and(|entry| entry.name == "center_divider")
        })
        .expect("a row for the divider");
    harness.send(Message::Palette(PaletteMsg::Pick(at))).await;
    assert!(harness.app.palette.is_none());
    assert_eq!(harness.app.resize.left_share(), 500);
    assert_eq!(load(db).await.1, 500.0);
}

/// What was read from the settings - at the start, after `settings_set` - is in force at once,
/// held to the area as measured.
#[tokio::test]
async fn the_read_settings_take_effect() {
    let (_root, _db, mut harness) = harness().await;
    let mut columns = ColumnWidths::default();
    columns.set(Column::Size, 150.0);
    harness.send(wrap(ResizeMsg::Loaded(columns, 300.0))).await;
    assert_eq!(harness.app.resize.columns.get(Column::Size), Some(150.0));
    assert_eq!(harness.app.resize.left_share(), 300);
    harness
        .send(wrap(ResizeMsg::Area(Size::new(608.0, 400.0))))
        .await;
    assert_eq!(
        harness.app.resize.left_share(),
        400,
        "600 for the panels: the left one keeps 240"
    );
}
