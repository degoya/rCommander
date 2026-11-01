//! Dracula (Dracula Theme, MIT, credited in `NOTICE`): the dark mode is Dracula, its terminal
//! Dracula's own; the light mode is Alucard, Dracula's light palette, whose terminal is derived
//! from it (the eight colours as normal and bright, Alucard's comment colour as bright black). The
//! syntax colours follow the Dracula specification (keywords and operators pink, keys and types
//! cyan, strings yellow, constants purple, functions green); the comment colour, below 4.5:1 on
//! the surface, is a little lighter.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xF0ECDB,
    surface: 0xFFFBEB,
    header: 0xF8F4E3,
    stroke: 0xDFDAC8,
    text: 0x1F1F1F,
    text_secondary: 0x6C664B,
    accent: 0x644AC9,
    cursor: 0xCFCFDE,
    cursor_inactive: 0xE5E3E4,
    selected: 0xF1EEE7,
    cursor_text: 0x1F1F1F,
    marked: 0xCB3A2A,
    list_fill: 0xFFFBEB,
    list_text: 0x1F1F1F,
    list_text_secondary: 0x6C664B,
    stripe: 0xFAF6E5,
    control: 0xF8F4E3,
    syntax: [
        0xA3144D, 0x036A96, 0x846E15, 0x644AC9, 0x644AC9, 0x6C664B, 0x1F1F1F, 0xA3144D, 0x036A96,
        0x14710A, 0xCB3A2A,
    ],
    terminal: [
        0x1F1F1F, 0xFFFBEB, 0x1F1F1F, 0xCB3A2A, 0x14710A, 0x846E15, 0x644AC9, 0xA3144D, 0x036A96,
        0xCFCFDE, 0x6C664B, 0xCB3A2A, 0x14710A, 0xA34D14, 0x644AC9, 0xA3144D, 0x036A96, 0xFFFBEB,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x21222C,
    surface: 0x282A36,
    header: 0x343746,
    stroke: 0x44475A,
    text: 0xF8F8F2,
    text_secondary: 0xCBD0DB,
    accent: 0xBD93F9,
    cursor: 0x44475A,
    cursor_inactive: 0x343746,
    selected: 0x333644,
    cursor_text: 0xF8F8F2,
    marked: 0xF1FA8C,
    list_fill: 0x282A36,
    list_text: 0xF8F8F2,
    list_text_secondary: 0xCBD0DB,
    stripe: 0x2D2F3C,
    control: 0x343746,
    syntax: [
        0xFF79C6, 0x8BE9FD, 0xF1FA8C, 0xBD93F9, 0xBD93F9, 0x8490B7, 0xF8F8F2, 0xFF79C6, 0x8BE9FD,
        0x50FA7B, 0xFF5555,
    ],
    terminal: [
        0xF8F8F2, 0x282A36, 0x21222C, 0xFF5555, 0x50FA7B, 0xF1FA8C, 0xBD93F9, 0xFF79C6, 0x8BE9FD,
        0xF8F8F2, 0x6272A4, 0xFF6E6E, 0x69FF94, 0xFFFFA5, 0xD6ACFF, 0xFF92DF, 0xA4FFFF, 0xFFFFFF,
    ],
};
