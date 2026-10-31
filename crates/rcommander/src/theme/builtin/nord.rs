//! Nord (Arctic Ice Studio, MIT, credited in `NOTICE`): Polar Night, Snow Storm, Frost and Aurora.
//! The dark mode is Nord as it ships, its terminal Nord's own; the light mode puts the Polar Night
//! text on Snow Storm. Syntax colours below 4.5:1 on the surface are a little lighter (dark) or
//! darker (light) than the palette's.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xD8DEE9,
    surface: 0xECEFF4,
    header: 0xE5E9F0,
    stroke: 0xCDD3DE,
    text: 0x2E3440,
    text_secondary: 0x4C566A,
    accent: 0x5E81AC,
    cursor: 0xC9D5E6,
    cursor_inactive: 0xDDE2EA,
    selected: 0xE2E6EE,
    cursor_text: 0x2E3440,
    marked: 0x8C3F47,
    list_fill: 0xECEFF4,
    list_text: 0x2E3440,
    list_text_secondary: 0x4C566A,
    stripe: 0xE6EAF1,
    control: 0xE5E9F0,
    syntax: [
        0x4F6C90, 0x546F6E, 0x607053, 0x7E6379, 0x586D83, 0x4C566A, 0x4C566A, 0x2E3440, 0x546F6E,
        0x4F6C90, 0xA2525A,
    ],
    terminal: [
        0x2E3440, 0xECEFF4, 0x3B4252, 0xA2525A, 0x607053, 0x786847, 0x4F6C90, 0x7E6379, 0x546F6E,
        0xD8DEE9, 0x4C566A, 0xBF616A, 0xA3BE8C, 0xD08770, 0x5E81AC, 0xB48EAD, 0x8FBCBB, 0xECEFF4,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x242933,
    surface: 0x2E3440,
    header: 0x3B4252,
    stroke: 0x434C5E,
    text: 0xECEFF4,
    text_secondary: 0xD8DEE9,
    accent: 0x88C0D0,
    cursor: 0x4C566A,
    cursor_inactive: 0x3B4252,
    selected: 0x353C4A,
    cursor_text: 0xECEFF4,
    marked: 0xEBCB8B,
    list_fill: 0x2E3440,
    list_text: 0xECEFF4,
    list_text_secondary: 0xD8DEE9,
    stripe: 0x323846,
    control: 0x3B4252,
    syntax: [
        0x81A1C1, 0x8FBCBB, 0xA3BE8C, 0xB793B0, 0x88C0D0, 0x959EAF, 0xECEFF4, 0x81A1C1, 0x8FBCBB,
        0x88C0D0, 0xD08C92,
    ],
    terminal: [
        0xD8DEE9, 0x2E3440, 0x3B4252, 0xBF616A, 0xA3BE8C, 0xEBCB8B, 0x81A1C1, 0xB48EAD, 0x88C0D0,
        0xE5E9F0, 0x4C566A, 0xBF616A, 0xA3BE8C, 0xEBCB8B, 0x81A1C1, 0xB48EAD, 0x8FBCBB, 0xECEFF4,
    ],
};
