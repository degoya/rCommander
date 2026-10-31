//! Solarized (Ethan Schoonover, MIT, credited in `NOTICE`): base03 to base3 and the eight accent
//! colours. The lists and the terminal take the palette as it is - the terminal's mapping is the
//! one the palette ships for terminals; the syntax colours that fall below 4.5:1 on the surface
//! are a little lighter (dark) or darker (light), and the light mode's text is base02 and base01
//! for the same reason.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xEEE8D5,
    surface: 0xFDF6E3,
    header: 0xF5EEDA,
    stroke: 0xD9D2BE,
    text: 0x073642,
    text_secondary: 0x586E75,
    accent: 0x268BD2,
    cursor: 0xE6DCC0,
    cursor_inactive: 0xEEE8D5,
    selected: 0xF6E9D0,
    cursor_text: 0x073642,
    marked: 0xB03A2E,
    list_fill: 0xFDF6E3,
    list_text: 0x073642,
    list_text_secondary: 0x586E75,
    stripe: 0xF7F0DC,
    control: 0xF5EEDA,
    syntax: [
        0x687700, 0x2073AE, 0x207C75, 0xC8337C, 0x6368B4, 0x586E75, 0x586E75, 0x073642, 0x8B6900,
        0x2073AE, 0xD3302D,
    ],
    terminal: [
        0x586E75, 0xFDF6E3, 0x073642, 0xDC322F, 0x859900, 0xB58900, 0x268BD2, 0xD33682, 0x2AA198,
        0xEEE8D5, 0x002B36, 0xCB4B16, 0x586E75, 0x657B83, 0x839496, 0x6C71C4, 0x93A1A1, 0xFDF6E3,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x00252E,
    surface: 0x002B36,
    header: 0x073642,
    stroke: 0x0E4A59,
    text: 0x93A1A1,
    text_secondary: 0x839496,
    accent: 0x268BD2,
    cursor: 0x0B4F63,
    cursor_inactive: 0x073642,
    selected: 0x08404F,
    cursor_text: 0xEEE8D5,
    marked: 0xD7A800,
    list_fill: 0x002B36,
    list_text: 0x93A1A1,
    list_text_secondary: 0x839496,
    stripe: 0x01313D,
    control: 0x073642,
    syntax: [
        0x859900, 0x3A95D6, 0x2AA198, 0xDE66A0, 0x8589CE, 0x839496, 0x93A1A1, 0x93A1A1, 0xB58900,
        0x3A95D6, 0xE56765,
    ],
    terminal: [
        0x839496, 0x002B36, 0x073642, 0xDC322F, 0x859900, 0xB58900, 0x268BD2, 0xD33682, 0x2AA198,
        0xEEE8D5, 0x002B36, 0xCB4B16, 0x586E75, 0x657B83, 0x839496, 0x6C71C4, 0x93A1A1, 0xFDF6E3,
    ],
};
