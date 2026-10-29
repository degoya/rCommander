//! The viewer's text size through `apply`, without a window (RC-020-12): Ctrl++, Ctrl+-, Ctrl+0
//! and Ctrl+wheel size the text of Lister and Quick View within 6 to 72 points, the size goes
//! along with N and P and with Quick View's cursor, the settings switch the keys off or store
//! the size, and the image view zooms with the same keys. The harness runs no tasks: the
//! settings are read with [`super::load`] and handed over.

use iced::{keyboard::Modifiers, widget::text_editor::Action, window};
use rc_db::Db;
use rc_engine::{
    Command, EngineEvent, PanelId,
    settings::{TEXT_ZOOM_KEEP_KEY, TEXT_ZOOM_KEY, VIEWER_FONT_SIZE_KEY},
    viewer::ViewMode,
};
use tempfile::TempDir;

use super::{TextSizeMsg, TextZoom, concerns, load, step};
use crate::{
    app::{
        Message,
        file_ops::FileOpsMsg,
        harness::{self, Harness},
        viewer::{QuickShown, ViewerMessage, lister::Lister},
    },
    i18n::t,
    theme,
};

/// `a.txt` and `b.txt`.
fn tree() -> TempDir {
    harness::tree(&[("a.txt", "alpha\nbeta\n"), ("b.txt", "gamma\n")])
}

/// Lister on `a.txt` over the main window (`..`, then `a.txt`).
async fn lister_on_a(harness: &mut Harness) -> u64 {
    harness.press("ArrowDown").await;
    let view = Command::ViewFile {
        panel: None,
        path: None,
    };
    harness
        .app
        .engine()
        .dispatch(view)
        .await
        .expect("announced");
    harness.settle().await;
    top(harness).id
}

fn top(harness: &Harness) -> &Lister {
    harness.app.viewers.open.last().expect("a Lister")
}

fn top_mut(harness: &mut Harness) -> &mut Lister {
    harness.app.viewers.open.last_mut().expect("a Lister")
}

/// The size of the settings, which a window without one of its own shows.
fn settings_size() -> f32 {
    theme::fonts::fonts().viewer.size
}

async fn hold_ctrl(harness: &mut Harness, held: bool) {
    let modifiers = if held {
        Modifiers::CTRL
    } else {
        Modifiers::empty()
    };
    harness
        .send(Message::FileOps(FileOpsMsg::Modifiers(modifiers)))
        .await;
    assert_eq!(harness.app.modifiers_held().ctrl, held);
}

async fn set_zoom(harness: &mut Harness, zoom: TextZoom) {
    let settings = ViewerMessage::TextSize(TextSizeMsg::Settings(zoom));
    harness.send(Message::Viewer(settings)).await;
}

/// The wheel in the text view's editor; `lines` below 0 is away from the person.
async fn wheel_in_text(harness: &mut Harness, id: u64, lines: i32) {
    let scroll = ViewerMessage::Editor(id, Action::Scroll { lines });
    harness.send(Message::Viewer(scroll)).await;
}

#[test]
fn the_steps_are_one_point_apart_up_to_16_and_end_at_6_and_72() {
    assert_eq!(step(13.0, true), 14.0);
    assert_eq!(step(13.0, false), 12.0);
    assert_eq!(step(16.0, true), 18.0);
    assert_eq!(step(18.0, false), 16.0);
    assert_eq!(step(48.0, true), 54.0);
    // A size of the settings between two steps goes to the next one.
    assert_eq!(step(17.0, true), 18.0);
    assert_eq!(step(17.0, false), 16.0);
    assert_eq!(step(72.0, true), 72.0, "the largest holds");
    assert_eq!(step(6.0, false), 6.0, "the smallest holds");
    assert_eq!(step(2.0, true), 6.0);
    assert_eq!(step(99.0, false), 72.0);
    assert!(concerns(TEXT_ZOOM_KEY) && concerns(TEXT_ZOOM_KEEP_KEY));
    assert!(!concerns(VIEWER_FONT_SIZE_KEY), "the fonts read that one");
}

#[tokio::test]
async fn ctrl_plus_minus_and_zero_size_the_text_and_say_so() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    let id = lister_on_a(&mut harness).await;
    let base = settings_size();
    assert_eq!(top(&harness).font().size, base);

    harness.press("Ctrl+Plus").await;
    let larger = step(base, true);
    assert_eq!(top(&harness).text_size, Some(larger));
    assert_eq!(top(&harness).font().size, larger);
    assert_eq!(
        top(&harness).note.as_deref(),
        Some(t!("viewer.text_size", size = larger).as_str())
    );
    harness.press("Ctrl+=").await;
    harness.press("Ctrl+NumAdd").await;
    let three = step(step(larger, true), true);
    assert_eq!(top(&harness).font().size, three);
    harness.press("Ctrl+-").await;
    harness.press("Ctrl+NumSubtract").await;
    assert_eq!(top(&harness).font().size, larger);

    harness.press("Ctrl+0").await;
    assert_eq!(top(&harness).text_size, None, "back to the settings");
    assert_eq!(top(&harness).font().size, base);
    assert_eq!(top(&harness).id, id, "the same window");
    // The panel underneath kept its keys' effects to itself: nothing got selected.
    assert!(harness.app.pane(PanelId::Left).state.selection.is_empty());
}

#[tokio::test]
async fn the_size_stays_within_6_and_72() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    lister_on_a(&mut harness).await;
    for _ in 0..40 {
        harness.press("Ctrl+Plus").await;
    }
    assert_eq!(top(&harness).font().size, 72.0);
    for _ in 0..60 {
        harness.press("Ctrl+-").await;
    }
    assert_eq!(top(&harness).font().size, 6.0);
}

#[tokio::test]
async fn ctrl_wheel_sizes_the_text_and_the_plain_wheel_still_scrolls() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    let id = lister_on_a(&mut harness).await;
    let base = settings_size();

    wheel_in_text(&mut harness, id, -4).await;
    assert_eq!(top(&harness).text_size, None, "without Ctrl it scrolls");

    hold_ctrl(&mut harness, true).await;
    assert!(harness.app.wheel_sizes_text());
    wheel_in_text(&mut harness, id, -4).await;
    assert_eq!(top(&harness).font().size, step(base, true), "away: larger");
    wheel_in_text(&mut harness, id, 4).await;
    wheel_in_text(&mut harness, id, 4).await;
    assert_eq!(
        top(&harness).font().size,
        step(base, false),
        "towards: smaller"
    );

    // The text view hears the wheel in its editor; the subscription's turn adds no second step.
    let turned = TextSizeMsg::Wheel(window::Id::unique(), 1.0);
    harness
        .send(Message::Viewer(ViewerMessage::TextSize(turned)))
        .await;
    assert_eq!(top(&harness).font().size, step(base, false));

    hold_ctrl(&mut harness, false).await;
    assert!(!harness.app.wheel_sizes_text());
}

#[tokio::test]
async fn n_and_p_keep_the_size_of_the_window() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    lister_on_a(&mut harness).await;
    harness.press("Ctrl+Plus").await;
    let size = top(&harness).text_size;
    harness.press("N").await;
    assert!(top(&harness).path.path.ends_with("b.txt"));
    assert_eq!(top(&harness).text_size, size);
    harness.press("P").await;
    assert!(top(&harness).path.path.ends_with("a.txt"));
    assert_eq!(top(&harness).text_size, size);
}

#[tokio::test]
async fn quick_view_with_the_keys_is_sized_and_keeps_it_as_the_cursor_moves() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+Q").await;
    harness.press("ArrowDown").await;
    // Tab into the Quick View: its viewer has the keys.
    harness.press("Tab").await;
    assert_eq!(harness.app.active, PanelId::Right);
    harness.press("Ctrl+Plus").await;
    let larger = Some(step(settings_size(), true));
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    assert_eq!(quick.text_size, larger);
    let QuickShown::File(lister) = &quick.shown else {
        panic!("a file is shown");
    };
    assert_eq!(lister.text_size, larger);

    harness.press("Tab").await;
    harness.press("ArrowDown").await;
    let quick = harness.app.viewers.quick.as_ref().expect("on");
    let QuickShown::File(lister) = &quick.shown else {
        panic!("a file is shown");
    };
    assert!(lister.path.path.ends_with("b.txt"));
    assert_eq!(lister.text_size, larger, "the next file at the same size");
}

#[tokio::test]
async fn switched_off_the_keys_do_nothing_and_quick_view_gives_them_to_the_panel() {
    let root = tree();
    let db = Db::open_in_memory().await.expect("db");
    db.set(TEXT_ZOOM_KEY, &false).await.expect("set");
    let mut harness = Harness::on_db(root.path(), db.clone()).await;
    let changed = Message::Engine(EngineEvent::SettingChanged {
        key: TEXT_ZOOM_KEY.to_owned(),
    });
    assert!(
        harness.app.viewer_intercept(&changed).is_some(),
        "read again"
    );
    let zoom = load(db).await;
    assert_eq!(
        zoom,
        TextZoom {
            enabled: false,
            save: false
        }
    );
    set_zoom(&mut harness, zoom).await;

    let id = lister_on_a(&mut harness).await;
    harness.press("Ctrl+Plus").await;
    harness.press("Ctrl+-").await;
    assert_eq!(top(&harness).text_size, None);
    hold_ctrl(&mut harness, true).await;
    assert!(!harness.app.wheel_sizes_text());
    wheel_in_text(&mut harness, id, -4).await;
    assert_eq!(top(&harness).text_size, None, "Ctrl+wheel scrolls");
    hold_ctrl(&mut harness, false).await;
    harness.press("Escape").await;

    // Quick View with the keys: Ctrl+NumAdd selects all in the panel, as without Quick View.
    harness.press("Ctrl+Q").await;
    harness.press("Tab").await;
    harness.press("Ctrl+NumAdd").await;
    assert!(
        !harness.app.pane(PanelId::Right).state.selection.is_empty(),
        "the panel's select all"
    );
}

#[tokio::test]
async fn kept_as_the_setting_the_size_is_stored_and_ctrl_0_stores_the_default() {
    let root = tree();
    let db = Db::open_in_memory().await.expect("db");
    db.set(TEXT_ZOOM_KEEP_KEY, &"setting").await.expect("set");
    let mut harness = Harness::on_db(root.path(), db.clone()).await;
    let zoom = load(db.clone()).await;
    assert!(zoom.enabled && zoom.save);
    set_zoom(&mut harness, zoom).await;
    lister_on_a(&mut harness).await;

    harness.press("Ctrl+Plus").await;
    let larger = step(settings_size(), true);
    let stored: Option<i64> = db.get(VIEWER_FONT_SIZE_KEY).await.expect("read");
    assert_eq!(stored, Some(larger as i64));
    assert_eq!(
        top(&harness).note.as_deref(),
        Some(t!("viewer.text_size_saved", size = larger).as_str())
    );
    harness.press("Ctrl+0").await;
    let stored: Option<i64> = db.get(VIEWER_FONT_SIZE_KEY).await.expect("read");
    assert_eq!(stored, Some(13), "the default size");
    assert_eq!(top(&harness).font().size, 13.0);
}

#[tokio::test]
async fn for_this_window_only_nothing_is_stored() {
    let root = tree();
    let db = Db::open_in_memory().await.expect("db");
    let mut harness = Harness::on_db(root.path(), db.clone()).await;
    let zoom = load(db.clone()).await;
    assert_eq!(zoom, TextZoom::default(), "on, for the window alone");
    set_zoom(&mut harness, zoom).await;
    lister_on_a(&mut harness).await;
    harness.press("Ctrl+Plus").await;
    harness.press("Ctrl+Plus").await;
    let stored: Option<i64> = db.get(VIEWER_FONT_SIZE_KEY).await.expect("read");
    assert_eq!(stored, None);
}

#[tokio::test]
async fn the_image_view_zooms_with_the_ctrl_keys_and_the_wheel() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    lister_on_a(&mut harness).await;
    top_mut(&mut harness).mode = ViewMode::Image;

    harness.press("Ctrl+Plus").await;
    let zoom = top(&harness).zoom;
    assert!(!zoom.fit);
    assert_eq!(zoom.scale, 1.0, "from fit to 100 %, as +");
    harness.press("Ctrl+=").await;
    assert_eq!(top(&harness).zoom.scale, 1.25);
    harness.press("Ctrl+-").await;
    assert_eq!(top(&harness).zoom.scale, 1.0);
    harness.press("R").await;
    harness.press("Ctrl+0").await;
    let zoom = top(&harness).zoom;
    assert!(zoom.fit, "fit to the window again");
    assert_eq!(zoom.turns, 1, "the turn stays");
    assert_eq!(top(&harness).text_size, None, "no text size");

    // The picture's scrollable keeps the wheel: the subscription hears it with Ctrl held.
    hold_ctrl(&mut harness, true).await;
    let turned = TextSizeMsg::Wheel(window::Id::unique(), 1.0);
    harness
        .send(Message::Viewer(ViewerMessage::TextSize(turned)))
        .await;
    assert!(!top(&harness).zoom.fit);
    let turned = TextSizeMsg::Wheel(window::Id::unique(), -1.0);
    harness
        .send(Message::Viewer(ViewerMessage::TextSize(turned)))
        .await;
    assert_eq!(top(&harness).zoom.scale, 1.0 / 1.25);
    hold_ctrl(&mut harness, false).await;

    // The keys without Ctrl are as before.
    harness.press("Ctrl+0").await;
    harness.press("NumAdd").await;
    assert_eq!(top(&harness).zoom.scale, 1.0);
    harness.press("Plus").await;
    assert_eq!(top(&harness).zoom.scale, 1.25);
}

#[test]
fn a_dos_line_is_as_tall_as_the_block_floored() {
    assert_eq!(
        super::tight_line(13.0, 1.16),
        15.0,
        "Cascadia Mono's block at 13"
    );
    assert_eq!(super::tight_line(13.0, 1.0), 13.0);
    assert!(
        super::tight_line(13.0, 1.16) < crate::theme::fonts::Fonts::DEFAULT.viewer.line_height(),
        "tighter than ordinary text"
    );
    assert_eq!(super::tight_line(0.5, 1.0), 1.0, "one pixel at least");
}
