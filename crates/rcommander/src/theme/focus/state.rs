//! A stop's state in the widget tree and the ring it draws (RC-080-16).

use iced::{
    Border, Color, Rectangle,
    advanced::{renderer, widget::operation::Focusable},
};

use super::{Kind, RING_GAP, RING_RADIUS, RING_WIDTH};

/// A stop's state in the widget tree: whether it has the keyboard, and whose it is; its access
/// key, and whether that key was pressed for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct State {
    pub(super) focused: bool,
    pub(super) key: u64,
    pub(super) kind: Kind,
    pub(super) letter: Option<char>,
    pub(super) armed: bool,
}

impl State {
    pub(super) fn new(key: u64, kind: Kind, letter: Option<char>) -> Self {
        Self {
            focused: false,
            key,
            kind,
            letter,
            armed: false,
        }
    }

    pub fn kind(&self) -> Kind {
        self.kind
    }

    pub fn letter(&self) -> Option<char> {
        self.letter
    }

    /// The stop presses itself on the key event that comes next, which the scope delivers.
    pub fn arm(&mut self, on: bool) {
        self.armed = on;
    }
}

impl Focusable for State {
    fn is_focused(&self) -> bool {
        self.focused
    }

    fn focus(&mut self) {
        self.focused = true;
    }

    fn unfocus(&mut self) {
        self.focused = false;
    }
}

/// The ring round `bounds` in `color`.
pub fn ring(bounds: Rectangle, color: Color) -> renderer::Quad {
    renderer::Quad {
        bounds: bounds.expand(RING_GAP),
        border: Border {
            color,
            width: RING_WIDTH,
            radius: RING_RADIUS.into(),
        },
        ..renderer::Quad::default()
    }
}
