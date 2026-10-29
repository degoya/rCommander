use super::{PADDING, SPACING, tab_width, visible_range};
use crate::theme::fonts::AreaFont;

/// Five tabs 40 wide; the overflow button 20 wide.
const WIDTHS: [f32; 5] = [40.0; 5];
const MORE: f32 = 20.0;
/// Room for two tabs and the overflow button: 40 + 2 + 40, then 2 and 20.
const TWO: f32 = 104.0;

#[test]
fn a_row_that_fits_shows_every_tab() {
    let all = 5.0 * 40.0 + 4.0 * SPACING;
    assert_eq!(visible_range(&WIDTHS, 4, all, MORE), 0..5);
    assert_eq!(visible_range(&WIDTHS, 0, 1000.0, MORE), 0..5);
    assert_eq!(visible_range(&[], 0, 0.0, MORE), 0..0);
    assert_eq!(visible_range(&[], 0, -5.0, MORE), 0..0);
}

#[test]
fn an_active_tab_among_the_first_keeps_the_row_from_its_start() {
    assert_eq!(visible_range(&WIDTHS, 0, TWO, MORE), 0..2);
    assert_eq!(visible_range(&WIDTHS, 1, TWO, MORE), 0..2);
    assert_eq!(visible_range(&WIDTHS, 1, TWO - 1.0, MORE), 1..2);
}

#[test]
fn an_active_tab_further_on_is_shown_with_those_before_it() {
    assert_eq!(visible_range(&WIDTHS, 2, TWO, MORE), 1..3);
    assert_eq!(visible_range(&WIDTHS, 4, TWO, MORE), 3..5);
    // Three fit: the run ends at the active tab.
    let three = TWO + 42.0;
    assert_eq!(visible_range(&WIDTHS, 3, three, MORE), 1..4);
}

#[test]
fn room_left_before_the_active_tab_goes_to_the_tabs_after_it() {
    // A wide first tab leaves no room before the second; the third still fits after it.
    let widths = [90.0, 40.0, 40.0, 40.0];
    assert_eq!(visible_range(&widths, 1, TWO, MORE), 1..3);
}

#[test]
fn the_active_tab_stays_even_when_it_alone_is_too_wide() {
    let widths = [40.0, 300.0, 40.0];
    assert_eq!(visible_range(&widths, 1, TWO, MORE), 1..2);
    assert_eq!(visible_range(&WIDTHS, 3, 10.0, MORE), 3..4);
    // An index past the end is the last tab.
    assert_eq!(visible_range(&WIDTHS, 9, TWO, MORE), 3..5);
}

#[test]
fn a_tab_is_its_title_and_its_padding_in_the_list_font() {
    let font = AreaFont {
        font: iced::Font::DEFAULT,
        size: 12.0,
        mono: false,
    };
    let short = tab_width("a", font);
    let long = tab_width("Documents", font);
    assert!(short > 2.0 * PADDING, "{short}");
    assert!(long > short + 30.0, "{short} {long}");
    let larger = tab_width("Documents", AreaFont { size: 18.0, ..font });
    assert!(larger > long, "{long} {larger}");
    let mono = tab_width("Documents", AreaFont { mono: true, ..font });
    assert!(mono > long, "{long} {mono}");
}
