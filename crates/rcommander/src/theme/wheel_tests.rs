use iced::mouse::ScrollDelta;

use super::rows;

fn pixels(y: f32) -> ScrollDelta {
    ScrollDelta::Pixels { x: 0.0, y }
}

#[test]
fn a_notch_of_the_wheel_is_three_rows() {
    assert_eq!(rows(ScrollDelta::Lines { x: 0.0, y: -1.0 }, 24.0), 3);
    assert_eq!(rows(ScrollDelta::Lines { x: 0.0, y: 1.0 }, 24.0), -3);
}

/// A touchpad's steps of a few pixels each rounded to nothing; they add up to a row now.
#[test]
fn small_touchpad_steps_add_up_to_rows() {
    let moved: Vec<isize> = (0..4).map(|_| rows(pixels(-8.0), 32.0)).collect();
    assert_eq!(moved, [0, 0, 0, 1]);
    // A step back is kept too, and taken back first.
    assert_eq!(rows(pixels(8.0), 32.0), 0);
    assert_eq!(rows(pixels(-8.0), 32.0), 0);
    assert_eq!(rows(pixels(32.0), 32.0), -1);
}
