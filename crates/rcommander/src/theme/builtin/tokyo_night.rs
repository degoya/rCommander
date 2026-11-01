//! Tokyo Night (Enkia, `tokyo-night-vscode-theme`, MIT, credited in `NOTICE`): the dark mode is
//! Tokyo Night, the light mode Tokyo Night Light - the theme's own light variant. Window, lists and
//! syntax take the themes' editor and token colours; the dark terminal is the theme's with the
//! editor's foreground and the doc comments' colour as bright black, so both read on its
//! background. The comment colours, below 4.5:1 on the surface, are a little lighter (dark) or
//! darker (light).

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xD6D8DF,
    surface: 0xE6E7ED,
    header: 0xDADCE3,
    stroke: 0xC1C2C7,
    text: 0x343B58,
    text_secondary: 0x4E5579,
    accent: 0x2959AA,
    cursor: 0xC3C6D1,
    cursor_inactive: 0xDCDEE3,
    selected: 0xE1E2E8,
    cursor_text: 0x343B58,
    marked: 0x8C4351,
    list_fill: 0xE6E7ED,
    list_text: 0x343B58,
    list_text_secondary: 0x4E5579,
    stripe: 0xE3E4EA,
    control: 0xDADCE3,
    syntax: [
        0x65359D, 0x33635C, 0x385F0D, 0x965027, 0x65359D, 0x656870, 0x343B58, 0x006C86, 0x006C86,
        0x2959AA, 0x942F2F,
    ],
    terminal: [
        0x343B58, 0xD6D8DF, 0x343B58, 0x8C4351, 0x33635C, 0x8F5E15, 0x2959AA, 0x7B43BA, 0x006C86,
        0x707280, 0x343B58, 0x8C4351, 0x33635C, 0x8F5E15, 0x2959AA, 0x7B43BA, 0x006C86, 0x707280,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x16161E,
    surface: 0x1A1B26,
    header: 0x1F2130,
    stroke: 0x363B54,
    text: 0xA9B1D6,
    text_secondary: 0x8289AE,
    accent: 0x7AA2F7,
    cursor: 0x2E3450,
    cursor_inactive: 0x202330,
    selected: 0x1E202E,
    cursor_text: 0xC0CAF5,
    marked: 0xE0AF68,
    list_fill: 0x1A1B26,
    list_text: 0xA9B1D6,
    list_text_secondary: 0x8289AE,
    stripe: 0x1C1D29,
    control: 0x202330,
    syntax: [
        0xBB9AF7, 0x73DACA, 0x9ECE6A, 0xFF9E64, 0xBB9AF7, 0x7982A8, 0xC0CAF5, 0x89DDFF, 0x0DB9D7,
        0x7AA2F7, 0xFF5370,
    ],
    terminal: [
        0xA9B1D6, 0x16161E, 0x363B54, 0xF7768E, 0x73DACA, 0xE0AF68, 0x7AA2F7, 0xBB9AF7, 0x7DCFFF,
        0x787C99, 0x646E9C, 0xF7768E, 0x73DACA, 0xE0AF68, 0x7AA2F7, 0xBB9AF7, 0x7DCFFF, 0xACB0D0,
    ],
};
