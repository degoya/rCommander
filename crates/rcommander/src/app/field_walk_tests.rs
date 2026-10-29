use rc_engine::Chord;

use super::{next, tab};

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

#[test]
fn tab_goes_forward_shift_tab_back_and_nothing_else_walks() {
    assert_eq!(tab(&chord("Tab")), Some(false));
    assert_eq!(tab(&chord("Shift+Tab")), Some(true));
    assert_eq!(
        tab(&chord("Ctrl+Tab")),
        None,
        "Ctrl+Tab is the panels' tab key"
    );
    assert_eq!(tab(&chord("Enter")), None);
}

#[test]
fn the_walk_goes_round_the_fields() {
    assert_eq!(next(2, Some(0), false), Some(1), "password to its repeat");
    assert_eq!(next(2, Some(1), false), Some(0), "round to the first");
    assert_eq!(
        next(2, Some(0), true),
        Some(1),
        "Shift+Tab round to the last"
    );
    assert_eq!(next(3, Some(2), true), Some(1));
    assert_eq!(next(3, None, false), Some(0), "nothing focused: the first");
    assert_eq!(next(3, None, true), Some(2), "or the last");
    assert_eq!(next(0, None, false), None, "a card without fields");
}
