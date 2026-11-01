//! Gruvbox (Pavel Pertsev, morhetz, MIT, credited in `NOTICE`): the dark0 to light4 greys and the
//! bright, neutral and faded accents. The dark mode is Gruvbox dark on dark0, the light mode Gruvbox
//! light on light0; the terminals are the palette's own, the bright accents on dark and the faded
//! on light. Syntax colours follow `gruvbox.vim` (keywords red, identifiers blue, strings green,
//! constants purple, types yellow; functions aqua, since a list cannot show the scheme's bold
//! green); those below 4.5:1 on the surface are a little lighter (dark) or darker (light) than the
//! palette's.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xEBDBB2,
    surface: 0xFBF1C7,
    header: 0xF2E5BC,
    stroke: 0xD5C4A1,
    text: 0x3C3836,
    text_secondary: 0x665C54,
    accent: 0xAF3A03,
    cursor: 0xD5C4A1,
    cursor_inactive: 0xEBDBB2,
    selected: 0xF2E5BC,
    cursor_text: 0x282828,
    marked: 0x9D0006,
    list_fill: 0xFBF1C7,
    list_text: 0x3C3836,
    list_text_secondary: 0x665C54,
    stripe: 0xF6EBC2,
    control: 0xF2E5BC,
    syntax: [
        0x9D0006, 0x076678, 0x75700E, 0x8F3F71, 0x8F3F71, 0x786B5E, 0x3C3836, 0x3C3836, 0x976211,
        0x417957, 0xCC241D,
    ],
    terminal: [
        0x3C3836, 0xFBF1C7, 0xFBF1C7, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A,
        0x7C6F64, 0x928374, 0x9D0006, 0x79740E, 0xB57614, 0x076678, 0x8F3F71, 0x427B58, 0x3C3836,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x1D2021,
    surface: 0x282828,
    header: 0x32302F,
    stroke: 0x504945,
    text: 0xEBDBB2,
    text_secondary: 0xBDAE93,
    accent: 0xFE8019,
    cursor: 0x504945,
    cursor_inactive: 0x3C3836,
    selected: 0x343230,
    cursor_text: 0xFBF1C7,
    marked: 0xFABD2F,
    list_fill: 0x282828,
    list_text: 0xEBDBB2,
    list_text_secondary: 0xBDAE93,
    stripe: 0x2D2C2C,
    control: 0x3C3836,
    syntax: [
        0xFB5440, 0x83A598, 0xB8BB26, 0xD3869B, 0xD3869B, 0x9A8C7E, 0xEBDBB2, 0xEBDBB2, 0xFABD2F,
        0x8EC07C, 0xFB5440,
    ],
    terminal: [
        0xEBDBB2, 0x282828, 0x282828, 0xCC241D, 0x98971A, 0xD79921, 0x458588, 0xB16286, 0x689D6A,
        0xA89984, 0x928374, 0xFB4934, 0xB8BB26, 0xFABD2F, 0x83A598, 0xD3869B, 0x8EC07C, 0xEBDBB2,
    ],
};
