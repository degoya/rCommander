use iced::{
    Background, Element,
    advanced::widget::{Tree, tree::Tag},
    widget::{button, text},
};

use super::{entry, strip, turn, underline};
use crate::theme::{
    focus::{Kind, State},
    tokens,
};

type Strip<'a> = Element<'a, u8>;

fn three<'a>(chosen: u8) -> Strip<'a> {
    strip(
        &[(1, "One"), (2, "Two"), (3, "Three")],
        chosen,
        "tabs",
        |tab| tab,
        tokens(false),
    )
}

/// The kinds of the stops in `tree`, and the buttons outside any stop.
fn survey(tree: &Tree, inside: bool, stops: &mut Vec<Kind>, loose: &mut usize) {
    let stop = Tag::of::<State>();
    let sample: Element<'_, u8> = button(text("x")).into();
    let button = Tree::new(sample.as_widget()).tag;
    if tree.tag == stop {
        stops.push(tree.state.downcast_ref::<State>().kind());
    }
    if !inside && tree.tag == button {
        *loose += 1;
    }
    for child in &tree.children {
        survey(child, inside || tree.tag == stop, stops, loose);
    }
}

/// The row of tabs is one stop of the ring, a choice: Tab reaches it once and the arrows work
/// it; its tab buttons are no stops of their own.
#[test]
fn the_strip_is_one_choice_stop_of_the_ring() {
    let strip = three(2);
    let tree = Tree::new(strip.as_widget());
    let (mut stops, mut loose) = (Vec::new(), 0);
    survey(&tree, false, &mut stops, &mut loose);
    assert_eq!(stops, vec![Kind::Choice]);
    assert_eq!(loose, 0, "every tab is inside the stop");
}

/// Ctrl+Tab turns to the next tab and from the last to the first; Ctrl+Shift+Tab back.
#[test]
fn the_tabs_turn_round() {
    let tabs = [1, 2, 3];
    assert_eq!(turn(&tabs, 1, false), 2);
    assert_eq!(turn(&tabs, 3, false), 1, "round to the first");
    assert_eq!(turn(&tabs, 1, true), 3, "back round to the last");
    assert_eq!(turn(&tabs, 2, true), 1);
    assert_eq!(turn(&tabs, 9, false), 1, "none of them: the first");
    assert_eq!(turn::<u8>(&[], 4, false), 4, "no tabs: as it was");
}

/// The chosen tab is in the text colour with the accent under it and no fill under the
/// pointer; the others are secondary and fill on hover.
#[test]
fn the_chosen_tab_is_marked_by_colour_and_bar() {
    for dark in [false, true] {
        let tokens = tokens(dark);
        let chosen = entry(tokens, true, button::Status::Hovered);
        assert_eq!(chosen.text_color, tokens.text);
        assert_eq!(chosen.background, None);
        let other = entry(tokens, false, button::Status::Active);
        assert_eq!(other.text_color, tokens.text_secondary);
        assert_eq!(other.background, None);
        let hovered = entry(tokens, false, button::Status::Hovered);
        assert_eq!(
            hovered.background,
            Some(Background::Color(tokens.cursor_fill_inactive))
        );
        assert_eq!(
            underline(tokens, true).background,
            Some(Background::Color(tokens.accent))
        );
        assert_eq!(underline(tokens, false).background, None);
    }
}
