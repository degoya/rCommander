//! Classic: Total Commander's look for the light mode - a white list, black text, a blue cursor,
//! marked names in red, the grey face of a classic Windows dialog - and Norton Commander's blue
//! panels with cyan names and yellow marks for the dark mode. The terminals are the classic
//! console's: the VGA colours on white, the CGA colours on black.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xF0F0F0,
    surface: 0xFFFFFF,
    header: 0xF0F0F0,
    stroke: 0xA0A0A0,
    text: 0x000000,
    text_secondary: 0x404040,
    accent: 0x0A5FC8,
    cursor: 0xA8C8F8,
    cursor_inactive: 0xD8D8D8,
    selected: 0xFFF0F0,
    cursor_text: 0x000000,
    marked: 0xA00000,
    list_fill: 0xFFFFFF,
    list_text: 0x000000,
    list_text_secondary: 0x404040,
    stripe: 0xF4F4F4,
    control: 0xF0F0F0,
    syntax: [
        0x0000FF, 0x0451A5, 0xA31515, 0x098658, 0xAF00DB, 0x008000, 0x616161, 0x1A1A1A, 0x267F99,
        0x795E26, 0xCD3131,
    ],
    terminal: [
        0x000000, 0xFFFFFF, 0x000000, 0x800000, 0x008000, 0x808000, 0x000080, 0x800080, 0x008080,
        0xC0C0C0, 0x808080, 0xFF0000, 0x00FF00, 0xFFFF00, 0x0000FF, 0xFF00FF, 0x00FFFF, 0xFFFFFF,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x101030,
    surface: 0x1C1C40,
    header: 0x24244C,
    stroke: 0x3C3C70,
    text: 0xE8E8F0,
    text_secondary: 0xB0B0C8,
    accent: 0x00C8C8,
    cursor: 0x008080,
    cursor_inactive: 0x305070,
    selected: 0x101090,
    cursor_text: 0xFFFFFF,
    marked: 0xFFFF00,
    list_fill: 0x000080,
    list_text: 0x00FFFF,
    list_text_secondary: 0x80D0D0,
    stripe: 0x00008C,
    control: 0x2A2A58,
    syntax: [
        0x569CD6, 0x9CDCFE, 0xCE9178, 0xB5CEA8, 0xC586C0, 0x73A85E, 0xC5C5C5, 0xD4D4D4, 0x4EC9B0,
        0xDCDCAA, 0xF36A6A,
    ],
    terminal: [
        0xC0C0C0, 0x000000, 0x000000, 0xAA0000, 0x00AA00, 0xAA5500, 0x0000AA, 0xAA00AA, 0x00AAAA,
        0xAAAAAA, 0x555555, 0xFF5555, 0x55FF55, 0xFFFF55, 0x5555FF, 0xFF55FF, 0x55FFFF, 0xFFFFFF,
    ],
};
