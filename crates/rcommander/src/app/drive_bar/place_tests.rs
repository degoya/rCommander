use iced::{Rectangle, Size};
use rc_engine::PanelId;

use super::{Button, MARGIN, card_height, id, place};
use crate::theme;

const WINDOW: Size = Size {
    width: 1200.0,
    height: 800.0,
};
const CARD: Size = Size {
    width: 360.0,
    height: 300.0,
};

fn button(x: f32, y: f32) -> Rectangle {
    Rectangle {
        x,
        y,
        width: 40.0,
        height: 20.0,
    }
}

#[test]
fn the_card_opens_just_under_its_button_at_the_button_s_left_edge() {
    let at = place(button(300.0, 40.0), CARD, WINDOW);
    assert_eq!(
        (at.x, at.y),
        (300.0, 62.0),
        "2 under the button's lower edge"
    );
    assert_eq!((at.width, at.height), (CARD.width, CARD.height));
}

#[test]
fn a_button_near_the_right_edge_moves_the_card_left_into_the_window() {
    let at = place(button(1100.0, 40.0), CARD, WINDOW);
    assert_eq!(at.x, WINDOW.width - MARGIN - CARD.width);
    assert_eq!(at.y, 62.0, "only moved sideways");
}

#[test]
fn a_card_too_tall_for_the_room_below_moves_up_and_never_past_the_top() {
    let at = place(button(10.0, 600.0), CARD, WINDOW);
    assert_eq!(at.y, WINDOW.height - MARGIN - CARD.height);
    assert_eq!(at.x, 10.0);

    let low = Size {
        width: 400.0,
        height: 200.0,
    };
    let at = place(button(0.0, 150.0), CARD, low);
    assert_eq!((at.x, at.y), (MARGIN, MARGIN), "the window's top left");
    assert_eq!(at.height, low.height - 2.0 * MARGIN, "cut to the window");
    assert_eq!(at.width, CARD.width);
}

#[test]
fn a_window_narrower_than_the_card_cuts_it_and_keeps_the_margin() {
    let narrow = Size {
        width: 300.0,
        height: 800.0,
    };
    let at = place(button(200.0, 40.0), CARD, narrow);
    assert_eq!(at.x, MARGIN);
    assert_eq!(at.width, narrow.width - 2.0 * MARGIN);
}

#[test]
fn the_card_s_height_grows_with_its_lines_up_to_its_limit() {
    let one = card_height(1, 440.0);
    assert_eq!(card_height(0, 440.0), one, "an empty menu shows one line");
    assert_eq!(card_height(3, 440.0), one + 2.0 * theme::ROW_HEIGHT);
    assert_eq!(card_height(100, 440.0), 440.0);
}

/// Every button has an id of its own, per panel; the same button the same id each frame.
#[test]
fn every_button_s_id_is_its_own() {
    let work = ["Work".to_owned()];
    let staging = ["Work".to_owned(), "Staging".to_owned()];
    let buttons = [
        Button::Line,
        Button::More,
        Button::Folder(&work),
        Button::Folder(&staging),
        Button::Drive("Work"),
        Button::Drive("C:"),
    ];
    let mut ids = Vec::new();
    for panel in [PanelId::Left, PanelId::Right] {
        for button in buttons {
            let made = id(panel, button);
            assert_eq!(made, id(panel, button));
            assert!(!ids.contains(&made), "{button:?} on {panel:?}");
            ids.push(made);
        }
    }
}
