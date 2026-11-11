//! The measures and the look of the bars' buttons (RC-110-15), from the settings both bars share,
//! as Total Commander's Change button bar dialog sets them: the symbol's size
//! (`buttonbar.icon_size`, 16 to 48 pixels), the room between the symbol and the button's frame
//! (`buttonbar.button_padding`; TC sets the button's size apart from the symbol's) and flat
//! buttons (`buttonbar.flat`, TC's "Flat icons"). A flat button has no frame and no fill; under the
//! pointer it shows the framed button's look, at the keyboard's cursor the accent's frame, so
//! hover and focus stay visible (`docs/design.md`). A flat button keeps its frame's room, so flat
//! and framed buttons are equally large and the bar does not jump under the pointer. Between two
//! buttons stays the bar's own small gap; TC has no setting for it, a separator of the `.bar` file
//! is the visible gap.

use iced::{Border, widget::container};

use super::BarSettings;
use crate::{app::drive_bar::button_style as framed, theme::Tokens};

/// A button's frame, counted on each side of its face.
pub const BORDER: f32 = 1.0;
/// The narrowest the vertical bar's room for a face gets: a button without a symbol shows words.
pub const MIN_VERTICAL_FACE: f32 = 24.0;

/// The measures of a button from the bars' settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measures {
    /// The symbol's side.
    pub icon: f32,
    /// The room around the symbol, inside the frame.
    pub padding: f32,
}

impl Measures {
    pub fn of(settings: &BarSettings) -> Self {
        Self {
            icon: f32::from(settings.icon_size),
            padding: f32::from(settings.padding),
        }
    }

    /// What a button adds to its face on each side: the room and the frame.
    pub fn around(self) -> f32 {
        self.padding + BORDER
    }

    /// The width of a button whose face is `face` wide.
    pub fn button(self, face: f32) -> f32 {
        face + 2.0 * self.around()
    }

    /// The height of a button in the vertical bar, `line` the height of a line of its words.
    pub fn vertical_height(self, line: f32) -> f32 {
        self.button(self.icon.max(line))
    }

    /// The vertical bar's width.
    pub fn vertical_width(self) -> f32 {
        self.button(self.icon.max(MIN_VERTICAL_FACE))
    }
}

/// A button's look: `focused` at the keyboard's cursor, `hovered` under the pointer, `flat` as
/// the setting says.
pub fn button_style(tokens: &Tokens, flat: bool, focused: bool, hovered: bool) -> container::Style {
    if !flat || focused || hovered {
        return framed(tokens, focused);
    }
    container::Style {
        background: None,
        border: Border::default().rounded(4.0).width(0.0),
        text_color: Some(tokens.text),
        ..container::Style::default()
    }
}

/// Whether `style` draws a frame or a fill.
#[cfg(test)]
pub fn drawn(style: &container::Style) -> bool {
    style.border.width > 0.0 || matches!(style.background, Some(iced::Background::Color(_)))
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
