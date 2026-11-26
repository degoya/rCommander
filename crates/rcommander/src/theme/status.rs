//! A look's status colours and its fields' border (RC-110-77): WinUI's critical, caution and
//! success colours of the look's mode, and a border for every input field, each moved towards
//! black (light) or white (dark) until it reads on every background of the look - its window,
//! surface, header and lists. A built-in theme and a person's own get them the same way: a theme
//! file names none of them, so no theme can lose them.

use iced::Color;

use super::{
    Tokens,
    colours::{MIN_CONTRAST, contrast},
    hex,
};

/// The smallest contrast WCAG 2's 1.4.11 asks of a control's border against what is around it.
pub const MIN_BORDER_CONTRAST: f32 = 3.0;

/// The status colours of a look, and the border of its input fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Status {
    /// An error, or a warning that something is lost for good: text, never a button's fill.
    pub danger: Color,
    /// A warning: something changed (the diff, the git marks).
    pub warning: Color,
    /// Success: something added (the diff, the git marks).
    pub success: Color,
    /// The border of a text field, a pick list or a check box while it has no focus (the
    /// focused one carries the accent).
    pub field: Color,
}

impl Status {
    /// WinUI's light colours (`SystemFillColorCritical`, `Caution`, `Success`), each at 4.5:1
    /// on white, and a field border at 3:1 on Fluent's window and surface - before they are
    /// fitted to a look ([`fit`]).
    pub const LIGHT: Self = Self {
        danger: hex(0xC42B1C, 1.0),
        warning: hex(0x9D5D00, 1.0),
        success: hex(0x0F7B0F, 1.0),
        field: hex(0x8A8A8A, 1.0),
    };

    /// WinUI's dark colours, and a field border at 3:1 on Fluent's dark window and surface -
    /// before they are fitted to a look ([`fit`]).
    pub const DARK: Self = Self {
        danger: hex(0xFF99A4, 1.0),
        warning: hex(0xFCE100, 1.0),
        success: hex(0x6CCB5F, 1.0),
        field: hex(0x9A9A9A, 1.0),
    };

    /// The colours of a light or `dark` look whose text and fields sit on `backgrounds`: the
    /// mode's own, each moved just far enough to read - text at [`MIN_CONTRAST`], the border at
    /// [`MIN_BORDER_CONTRAST`].
    #[must_use]
    pub fn of(dark: bool, backgrounds: &[Color]) -> Self {
        let base = if dark { Self::DARK } else { Self::LIGHT };
        let text = |colour| readable(colour, dark, backgrounds, MIN_CONTRAST);
        Self {
            danger: text(base.danger),
            warning: text(base.warning),
            success: text(base.success),
            field: readable(base.field, dark, backgrounds, MIN_BORDER_CONTRAST),
        }
    }
}

/// The backgrounds a look's status text and fields sit on: its window, surface, header and
/// lists.
#[must_use]
pub fn backgrounds(tokens: &Tokens) -> [Color; 4] {
    [
        tokens.window,
        tokens.surface,
        tokens.header,
        tokens.list_fill,
    ]
}

/// `tokens` with the status colours of its mode fitted to its backgrounds: every look's last
/// step, again after a theme's own colours are laid over it (`themes.rs`).
pub(super) fn fit(tokens: &mut Tokens) {
    tokens.status = Status::of(tokens.dark, &backgrounds(tokens));
}

/// The steps from a colour to black or white.
const STEPS: u16 = 20;

/// `base`, moved in twentieths towards white (`dark`) or black until it reaches `minimum` on each
/// of `backgrounds`; white or black where nothing short of it does. The backgrounds are opaque.
#[must_use]
pub fn readable(base: Color, dark: bool, backgrounds: &[Color], minimum: f32) -> Color {
    let target = if dark { Color::WHITE } else { Color::BLACK };
    let mix = |from: f32, to: f32, share: f32| from + (to - from) * share;
    (0..=STEPS)
        .map(|step| {
            let share = f32::from(step) / f32::from(STEPS);
            Color::from_rgb(
                mix(base.r, target.r, share),
                mix(base.g, target.g, share),
                mix(base.b, target.b, share),
            )
        })
        .find(|colour| {
            backgrounds
                .iter()
                .all(|back| contrast(*colour, *back) >= minimum)
        })
        .unwrap_or(target)
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;
