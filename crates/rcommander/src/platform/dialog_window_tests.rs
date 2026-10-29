use super::{Rect, clamp};

const fn rect(x: i32, y: i32, width: i32, height: i32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// A primary monitor of 1920 x 1040 (taskbar below) and a second one at its right, 2560 x 1400.
const AREAS: [Rect; 2] = [rect(0, 0, 1920, 1040), rect(1920, 0, 2560, 1400)];

#[test]
fn a_window_on_a_monitor_stays_where_it_is() {
    let window = rect(200, 100, 900, 640);
    assert_eq!(clamp(window, &AREAS), window);
    let on_second = rect(2400, 300, 900, 640);
    assert_eq!(clamp(on_second, &AREAS), on_second);
}

#[test]
fn a_window_whose_title_bar_can_still_be_grabbed_stays() {
    // Mostly beyond the right edge, but 200 px of its title bar on the second monitor.
    let window = rect(4280, 500, 900, 640);
    assert_eq!(clamp(window, &AREAS), window);
}

#[test]
fn a_window_with_its_title_bar_above_every_monitor_comes_down() {
    let window = rect(300, -400, 900, 640);
    assert_eq!(clamp(window, &AREAS), rect(300, 0, 900, 640));
}

#[test]
fn a_window_on_a_monitor_that_is_gone_moves_to_the_primary() {
    // It was on a third monitor at the left that is no longer there.
    let window = rect(-1800, 200, 900, 640);
    assert_eq!(clamp(window, &AREAS), rect(0, 200, 900, 640));
}

#[test]
fn a_window_larger_than_the_monitor_it_lands_on_shrinks_to_it() {
    let window = rect(-3000, -2000, 2400, 1600);
    assert_eq!(clamp(window, &AREAS), rect(0, 0, 1920, 1040));
}

#[test]
fn a_window_moves_to_the_monitor_it_overlaps_most() {
    // Its title bar is above the screens; most of its body is on the second monitor.
    let window = rect(2000, -100, 900, 640);
    assert_eq!(clamp(window, &AREAS), rect(2000, 0, 900, 640));
}

#[test]
fn without_monitors_nothing_moves() {
    let window = rect(-5000, -5000, 900, 640);
    assert_eq!(clamp(window, &[]), window);
}
