//! Catppuccin (Catppuccin, MIT, credited in `NOTICE`): Latte for the light mode, Mocha for the
//! dark, each with the palette's ANSI colours for its terminal and mauve as the accent. The syntax
//! colours follow the style guide (keywords mauve, keys lavender, strings green, numbers peach,
//! operators sky, types yellow, functions blue); Latte's, most of them below 4.5:1 on its base,
//! are darker than the palette's.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xDCE0E8,
    surface: 0xEFF1F5,
    header: 0xE6E9EF,
    stroke: 0xCCD0DA,
    text: 0x4C4F69,
    text_secondary: 0x5C5F77,
    accent: 0x8839EF,
    cursor: 0xCCD0DA,
    cursor_inactive: 0xDCE0E8,
    selected: 0xE1E4EA,
    cursor_text: 0x4C4F69,
    marked: 0xD20F39,
    list_fill: 0xEFF1F5,
    list_text: 0x4C4F69,
    list_text_secondary: 0x5C5F77,
    stripe: 0xEAEDF2,
    control: 0xE6E9EF,
    syntax: [
        0x8839EF, 0x3F5BFC, 0x327D22, 0xBF4601, 0xBF4601, 0x6A6D81, 0x6A6D81, 0x0375A3, 0x996214,
        0x1862F5, 0xD20F39,
    ],
    terminal: [
        0x4C4F69, 0xEFF1F5, 0x5C5F77, 0xD20F39, 0x40A02B, 0xDF8E1D, 0x1E66F5, 0xEA76CB, 0x179299,
        0xACB0BE, 0x6C6F85, 0xDE293E, 0x49AF3D, 0xEEA02D, 0x456EFF, 0xFE85D8, 0x2D9FA8, 0xBCC0CC,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x11111B,
    surface: 0x1E1E2E,
    header: 0x313244,
    stroke: 0x45475A,
    text: 0xCDD6F4,
    text_secondary: 0xA6ADC8,
    accent: 0xCBA6F7,
    cursor: 0x45475A,
    cursor_inactive: 0x313244,
    selected: 0x262637,
    cursor_text: 0xCDD6F4,
    marked: 0xFAB387,
    list_fill: 0x1E1E2E,
    list_text: 0xCDD6F4,
    list_text_secondary: 0xA6ADC8,
    stripe: 0x222232,
    control: 0x313244,
    syntax: [
        0xCBA6F7, 0xB4BEFE, 0xA6E3A1, 0xFAB387, 0xFAB387, 0x9399B2, 0x9399B2, 0x89DCEB, 0xF9E2AF,
        0x89B4FA, 0xF38BA8,
    ],
    terminal: [
        0xCDD6F4, 0x1E1E2E, 0x45475A, 0xF38BA8, 0xA6E3A1, 0xF9E2AF, 0x89B4FA, 0xF5C2E7, 0x94E2D5,
        0xA6ADC8, 0x585B70, 0xF37799, 0x89D88B, 0xEBD391, 0x74A8FC, 0xF2AEDE, 0x6BD7CA, 0xBAC2DE,
    ],
};
