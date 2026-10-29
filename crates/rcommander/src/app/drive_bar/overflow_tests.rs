use super::{SPACING, button_width, visible_count};

#[test]
fn a_bar_that_fits_shows_every_button_and_no_overflow() {
    let widths = [40.0, 40.0, 40.0];
    assert_eq!(visible_count(&widths, 120.0 + 2.0 * SPACING, 20.0), 3);
    assert_eq!(visible_count(&widths, 500.0, 20.0), 3);
    assert_eq!(visible_count(&[], 0.0, 20.0), 0);
}

#[test]
fn a_bar_too_narrow_keeps_the_first_buttons_and_room_for_the_overflow() {
    let widths = [40.0, 40.0, 40.0, 40.0];
    // 40 + 3 + 40 = 83, then 3 and the 20-wide overflow: 106.
    assert_eq!(visible_count(&widths, 106.0, 20.0), 2);
    assert_eq!(visible_count(&widths, 105.0, 20.0), 1);
    assert_eq!(visible_count(&widths, 150.0, 20.0), 3);
    assert_eq!(visible_count(&widths, 10.0, 20.0), 0);
}

#[test]
fn a_wider_label_makes_a_wider_button() {
    let short = button_width(&["\u{1f5b4}", "C"], 12.0);
    let long = button_width(&["\u{1f5b4}", "Ubuntu-24.04"], 12.0);
    assert!(long > short + 40.0, "{short} {long}");
    assert!(short > 16.0, "padding and symbol: {short}");
}
