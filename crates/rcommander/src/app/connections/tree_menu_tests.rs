use iced::{Point, Rectangle};

use super::{MENU_WIDTH, spot};
use crate::theme::ROW_HEIGHT;

/// The tree's side at (20, 30), 400 by 600; its list from 120 down, 400 high.
fn side() -> Rectangle {
    Rectangle::new(Point::new(20.0, 30.0), iced::Size::new(400.0, 600.0))
}

fn list() -> Rectangle {
    Rectangle::new(Point::new(20.0, 120.0), iced::Size::new(400.0, 400.0))
}

/// The menu opens just under its row, beside the row's sign, in the side's coordinates.
#[test]
fn the_menu_stands_under_its_row() {
    let at = spot(side(), list(), 0.0, (2, 1), 10);
    let pitch = ROW_HEIGHT + 1.0;
    assert_eq!(at.x, 16.0 + 24.0);
    assert_eq!(at.y, 120.0 - 30.0 + 3.0 * pitch);
}

/// A scrolled list moves the row up; a row scrolled out of sight puts the menu at the list's
/// top.
#[test]
fn a_scrolled_list_is_taken_into_account() {
    let pitch = ROW_HEIGHT + 1.0;
    let at = spot(side(), list(), 10.0 * pitch, (12, 0), 10);
    assert_eq!(at.y, 120.0 - 30.0 + 3.0 * pitch);
    let gone = spot(side(), list(), 40.0 * pitch, (2, 0), 10);
    assert_eq!(gone.y, 120.0 - 30.0);
}

/// Near the side's bottom the menu moves up so it fits; a deep row does not push it out to the
/// right.
#[test]
fn the_menu_stays_inside_the_side() {
    let near_bottom = spot(side(), list(), 0.0, (14, 0), 10);
    let card = 10.0 * ROW_HEIGHT + 10.0;
    assert_eq!(near_bottom.y, 600.0 - card);
    let deep = spot(side(), list(), 0.0, (0, 30), 10);
    assert_eq!(deep.x, 400.0 - MENU_WIDTH);
}
