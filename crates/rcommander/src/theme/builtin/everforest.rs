//! Everforest (sainnhe, <https://github.com/sainnhe/everforest>, MIT, credited in `NOTICE`): the
//! medium-contrast light and dark palettes with Everforest's own syntax groups and terminal
//! mapping. Syntax colours below 4.5:1 on the surface are a little lighter (dark) or darker
//! (light) than the palette's; so are the secondary text, the marked name and the cursor's text
//! where they fall short on their rows.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xEFEBD4,
    surface: 0xFDF6E3,
    header: 0xF4F0D9,
    stroke: 0xE0DCC7,
    text: 0x5C6A72,
    text_secondary: 0x606D75,
    accent: 0x8DA101,
    cursor: 0xE6E2CC,
    cursor_inactive: 0xEFEBD4,
    selected: 0xF4F0D9,
    cursor_text: 0x58666D,
    marked: 0xBE413F,
    list_fill: 0xFDF6E3,
    list_text: 0x5C6A72,
    list_text_secondary: 0x606D75,
    stripe: 0xF8F3DE,
    control: 0xF4F0D9,
    syntax: [
        0xC44341, 0x2E769E, 0x687701, 0xA9508D, 0x287D5D, 0x6A7268, 0x5C6A72, 0xAE591B, 0x916800,
        0x687701, 0xC44341,
    ],
    terminal: [
        0x5C6A72, 0xFDF6E3, 0x5C6A72, 0xF85552, 0x8DA101, 0xDFA000, 0x3A94C5, 0xDF69BA, 0x35A77C,
        0xE6E2CC, 0x5C6A72, 0xF85552, 0x8DA101, 0xDFA000, 0x3A94C5, 0xDF69BA, 0x35A77C, 0xE6E2CC,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x232A2E,
    surface: 0x2D353B,
    header: 0x343F44,
    stroke: 0x475258,
    text: 0xD3C6AA,
    text_secondary: 0xA0ACA3,
    accent: 0xA7C080,
    cursor: 0x4F585E,
    cursor_inactive: 0x3D484D,
    selected: 0x343F44,
    cursor_text: 0xD7CCB2,
    marked: 0xDBBC7F,
    list_fill: 0x2D353B,
    list_text: 0xD3C6AA,
    list_text_secondary: 0xA0ACA3,
    stripe: 0x303A40,
    control: 0x343F44,
    syntax: [
        0xE67E80, 0x7FBBB3, 0xA7C080, 0xD699B6, 0x83C092, 0x949F97, 0xD3C6AA, 0xE69875, 0xDBBC7F,
        0xA7C080, 0xE67E80,
    ],
    terminal: [
        0xD3C6AA, 0x2D353B, 0x475258, 0xE67E80, 0xA7C080, 0xDBBC7F, 0x7FBBB3, 0xD699B6, 0x83C092,
        0xD3C6AA, 0x475258, 0xE67E80, 0xA7C080, 0xDBBC7F, 0x7FBBB3, 0xD699B6, 0x83C092, 0xD3C6AA,
    ],
};
