//! The mouse wheel and the touchpad as whole rows (RC-080-44). A notch of the wheel is three
//! rows, a touchpad's pixels are rows of the list's height; the part of a row an event leaves
//! over is kept for the next one, so a touchpad's small steps - a fraction of a row each, which
//! rounding made nothing of - add up to rows.

use std::cell::Cell;

use iced::mouse::ScrollDelta;

/// The rows a notch of the wheel scrolls.
pub const NOTCH_ROWS: f32 = 3.0;

thread_local! {
    /// The part of a row the last events left over; one list scrolls at a time, on the
    /// window's thread.
    static LEFT_OVER: Cell<f32> = const { Cell::new(0.0) };
}

/// The whole rows `delta` scrolls a list of rows `row_height` high by, downwards positive.
pub fn rows(delta: ScrollDelta, row_height: f32) -> isize {
    let rows = match delta {
        ScrollDelta::Lines { y, .. } => -y * NOTCH_ROWS,
        ScrollDelta::Pixels { y, .. } => -y / row_height.max(1.0),
    };
    LEFT_OVER.with(|left| {
        let total = left.get() + rows;
        if !total.is_finite() {
            left.set(0.0);
            return 0;
        }
        let whole = total.trunc();
        left.set(total - whole);
        // A float cast saturates: no wheel turns that far.
        whole as isize
    })
}

#[cfg(test)]
#[path = "wheel_tests.rs"]
mod tests;
