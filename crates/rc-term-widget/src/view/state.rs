//! What the widget remembers between events.

use alacritty_terminal::index::Point as GridPoint;
use iced::{Size, keyboard::Modifiers};
use iced_core::{
    mouse::{self, Click},
    widget::operation::Focusable,
};

use super::other_button;

/// The state of one [`super::TerminalView`] in the widget tree.
#[derive(Debug, Clone)]
pub(super) struct ViewState {
    pub(super) focus: bool,
    /// The focus the cached screen was drawn with (the cursor is hollow without it).
    pub(super) drawn_focus: bool,
    pub(super) dragging: bool,
    /// The drag goes to the program, not into a selection.
    pub(super) reporting: bool,
    /// The middle and right buttons whose press the program was told of, a bit each by
    /// xterm's number: their release goes to it too.
    pub(super) held: u8,
    pub(super) last_click: Option<Click>,
    /// Wheel pixels short of a line.
    pub(super) scroll_pixels: f32,
    pub(super) modifiers: Modifiers,
    /// The size and cell the grid was last fitted to.
    pub(super) size: Size,
    pub(super) cell: Size,
    /// The cell under the mouse.
    pub(super) grid_point: GridPoint,
}

impl Default for ViewState {
    fn default() -> Self {
        Self {
            focus: false,
            drawn_focus: false,
            dragging: false,
            reporting: false,
            held: 0,
            last_click: None,
            scroll_pixels: 0.0,
            modifiers: Modifiers::empty(),
            size: Size::new(0.0, 0.0),
            cell: Size::new(0.0, 0.0),
            grid_point: GridPoint::default(),
        }
    }
}

impl ViewState {
    /// `button` came up: whether the program was told of its press, which is forgotten with
    /// this. Only the middle and the right button are ever held so.
    pub(super) fn release(&mut self, button: mouse::Button) -> bool {
        if !matches!(button, mouse::Button::Middle | mouse::Button::Right) {
            return false;
        }
        let bit = 1 << other_button(button);
        let held = self.held & bit != 0;
        self.held &= !bit;
        held
    }
}

impl Focusable for ViewState {
    fn is_focused(&self) -> bool {
        self.focus
    }

    fn focus(&mut self) {
        self.focus = true;
    }

    fn unfocus(&mut self) {
        self.focus = false;
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
