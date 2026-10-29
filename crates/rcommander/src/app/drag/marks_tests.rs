//! Marking rows with the mouse through `apply`, without a window (RC-010-02): Ctrl, Shift and
//! Ctrl+Shift with a click, the plain click with `mouse.click_clears_marks` off and on, the `..`
//! row, the brief view, and no drag from a click with Ctrl or Shift.
//!
//! The listing of `tree()`: row 0 is `..`, then `inner`, `outer`, `f00.txt` ... `f39.txt`.

use iced::{Point, keyboard::Modifiers};
use rc_engine::PanelId;

use crate::app::{
    Message,
    drag::DragMsg,
    file_ops::FileOpsMsg,
    harness::Harness,
    tests::{cursor_name, tree},
};

const LEFT: PanelId = PanelId::Left;

/// A click on `row` of the left panel with `modifiers` held, released without a move.
async fn click(harness: &mut Harness, modifiers: Modifiers, row: usize) {
    hold(harness, modifiers).await;
    harness.send(Message::Click(LEFT, row)).await;
    harness.send(Message::Drag(DragMsg::Released)).await;
    hold(harness, Modifiers::empty()).await;
}

async fn hold(harness: &mut Harness, modifiers: Modifiers) {
    harness
        .send(Message::FileOps(FileOpsMsg::Modifiers(modifiers)))
        .await;
}

/// The marked names in the list's order.
fn marked(harness: &Harness) -> Vec<String> {
    let pane = harness.app.pane(LEFT);
    pane.entries()
        .iter()
        .filter(|entry| pane.state.selection.contains(&entry.name))
        .map(|entry| entry.name.clone())
        .collect()
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|name| (*name).to_owned()).collect()
}

#[tokio::test]
async fn ctrl_click_marks_or_unmarks_one_row_and_keeps_the_others() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    click(&mut harness, Modifiers::CTRL, 3).await;
    click(&mut harness, Modifiers::CTRL, 5).await;
    assert_eq!(marked(&harness), names(&["f00.txt", "f02.txt"]));
    assert_eq!(cursor_name(&harness, LEFT), "f02.txt");
    click(&mut harness, Modifiers::CTRL, 3).await;
    assert_eq!(marked(&harness), names(&["f02.txt"]));
    assert_eq!(cursor_name(&harness, LEFT), "f00.txt", "the cursor follows");
}

#[tokio::test]
async fn shift_click_marks_the_range_from_the_anchor_either_way() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    click(&mut harness, Modifiers::empty(), 3).await;
    click(&mut harness, Modifiers::SHIFT, 6).await;
    assert_eq!(
        marked(&harness),
        names(&["f00.txt", "f01.txt", "f02.txt", "f03.txt"])
    );
    assert_eq!(cursor_name(&harness, LEFT), "f03.txt");
    // The anchor stays on `f00.txt`; the new range replaces the old one.
    click(&mut harness, Modifiers::SHIFT, 1).await;
    assert_eq!(marked(&harness), names(&["inner", "outer", "f00.txt"]));
    assert_eq!(cursor_name(&harness, LEFT), "inner");
}

#[tokio::test]
async fn without_a_click_the_cursor_is_the_anchor_and_the_parent_row_is_never_marked() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    assert!(harness.app.pane(LEFT).on_parent());
    click(&mut harness, Modifiers::SHIFT, 4).await;
    assert_eq!(
        marked(&harness),
        names(&["inner", "outer", "f00.txt", "f01.txt"])
    );
    assert!(!harness.app.pane(LEFT).is_selected(0));
    click(&mut harness, Modifiers::CTRL, 0).await;
    let pane = harness.app.pane(LEFT);
    assert!(!pane.is_selected(0), "Ctrl+click on `..` marks nothing");
    assert!(pane.on_parent(), "and puts the cursor there");
    assert_eq!(pane.selected_count(), 4);
}

#[tokio::test]
async fn ctrl_shift_click_adds_the_range_to_the_marks() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    click(&mut harness, Modifiers::CTRL, 3).await;
    click(&mut harness, Modifiers::CTRL, 11).await;
    // From the anchor `f08.txt`, marked already, to `f10.txt`: it stays marked.
    click(&mut harness, Modifiers::CTRL | Modifiers::SHIFT, 13).await;
    assert_eq!(
        marked(&harness),
        names(&["f00.txt", "f08.txt", "f09.txt", "f10.txt"])
    );
    assert_eq!(cursor_name(&harness, LEFT), "f10.txt");
    // A plain Shift+click from the same anchor replaces them all.
    click(&mut harness, Modifiers::SHIFT, 12).await;
    assert_eq!(marked(&harness), names(&["f08.txt", "f09.txt"]));
}

#[tokio::test]
async fn a_plain_click_keeps_the_marks_unless_the_setting_clears_them() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    click(&mut harness, Modifiers::CTRL, 3).await;
    click(&mut harness, Modifiers::CTRL, 4).await;
    click(&mut harness, Modifiers::empty(), 8).await;
    assert_eq!(marked(&harness), names(&["f00.txt", "f01.txt"]), "as in TC");
    assert_eq!(cursor_name(&harness, LEFT), "f05.txt");

    harness
        .send(Message::Drag(DragMsg::ClickClears(true)))
        .await;
    // A press on a marked row that becomes a drag keeps the marks for the drag.
    harness.send(Message::Click(LEFT, 3)).await;
    harness
        .send(Message::Drag(DragMsg::Moved(Point::new(100.0, 100.0))))
        .await;
    harness
        .send(Message::Drag(DragMsg::Moved(Point::new(140.0, 100.0))))
        .await;
    harness.send(Message::Drag(DragMsg::Released)).await;
    assert_eq!(marked(&harness), names(&["f00.txt", "f01.txt"]));
    // The same press ending without a drag clears them, as in Explorer.
    click(&mut harness, Modifiers::empty(), 3).await;
    assert!(marked(&harness).is_empty());
    assert_eq!(cursor_name(&harness, LEFT), "f00.txt");
    // A click on an unmarked row clears them at once.
    click(&mut harness, Modifiers::CTRL, 3).await;
    click(&mut harness, Modifiers::CTRL, 4).await;
    harness.send(Message::Click(LEFT, 8)).await;
    assert!(marked(&harness).is_empty());
    // Ctrl and Shift mark as before.
    harness.send(Message::Drag(DragMsg::Released)).await;
    click(&mut harness, Modifiers::CTRL, 3).await;
    click(&mut harness, Modifiers::SHIFT, 4).await;
    assert_eq!(marked(&harness), names(&["f00.txt", "f01.txt"]));
}

#[tokio::test]
async fn a_click_with_ctrl_or_shift_starts_no_drag() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.send(Message::Click(LEFT, 3)).await;
    assert!(
        harness.app.drag.tracking(),
        "a plain press may become a drag"
    );
    harness.send(Message::Drag(DragMsg::Released)).await;
    for modifiers in [Modifiers::CTRL, Modifiers::SHIFT] {
        hold(&mut harness, modifiers).await;
        harness.send(Message::Click(LEFT, 4)).await;
        assert!(!harness.app.drag.tracking());
    }
}

#[tokio::test]
async fn the_brief_view_marks_by_the_rows_in_its_order() {
    let root = tree();
    let mut harness = Harness::headless(root.path()).await;
    harness.press("Ctrl+F1").await;
    assert!(harness.app.pane(LEFT).brief());
    click(&mut harness, Modifiers::empty(), 2).await;
    click(&mut harness, Modifiers::SHIFT, 30).await;
    let range = marked(&harness);
    assert_eq!(range.len(), 29);
    assert_eq!(range.first().map(String::as_str), Some("outer"));
    assert_eq!(range.last().map(String::as_str), Some("f27.txt"));
    click(&mut harness, Modifiers::CTRL, 30).await;
    assert_eq!(marked(&harness).len(), 28);
    assert_eq!(cursor_name(&harness, LEFT), "f27.txt");
}
