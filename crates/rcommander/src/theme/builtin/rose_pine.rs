//! Rose Pine (Rose Pine, <https://github.com/rose-pine/neovim>, MIT, credited in `NOTICE`): the
//! main variant for the dark mode and Dawn for the light, with the syntax groups of the Neovim
//! theme and the terminal of the Alacritty port (<https://github.com/rose-pine/alacritty>). The
//! palette gives strings and numbers the same gold; numbers take iris here. Syntax colours below
//! 4.5:1 on the surface are a little lighter (dark) or darker (light) than the palette's; so are
//! Dawn's secondary text and marked name.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xF2E9E1,
    surface: 0xFAF4ED,
    header: 0xFFFAF3,
    stroke: 0xDFDAD9,
    text: 0x464261,
    text_secondary: 0x6B6881,
    accent: 0xD7827E,
    cursor: 0xDFDAD9,
    cursor_inactive: 0xF2E9E1,
    selected: 0xF4EDE8,
    cursor_text: 0x464261,
    marked: 0x9D566B,
    list_fill: 0xFAF4ED,
    list_text: 0x464261,
    list_text_secondary: 0x6B6881,
    stripe: 0xF7F0EA,
    control: 0xFFFAF3,
    syntax: [
        0x286983, 0x45767F, 0x966421, 0x7A6890, 0x9D5F5C, 0x6F6C87, 0x6F6C87, 0x6F6C87, 0x45767F,
        0x9D5F5C, 0xA2596E,
    ],
    terminal: [
        0x575279, 0xFAF4ED, 0xF2E9E1, 0xB4637A, 0x286983, 0xEA9D34, 0x56949F, 0x907AA9, 0xD7827E,
        0x575279, 0x9893A5, 0xB4637A, 0x286983, 0xEA9D34, 0x56949F, 0x907AA9, 0xD7827E, 0x575279,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x16141F,
    surface: 0x191724,
    header: 0x1F1D2E,
    stroke: 0x403D52,
    text: 0xE0DEF4,
    text_secondary: 0x908CAA,
    accent: 0xEBBCBA,
    cursor: 0x403D52,
    cursor_inactive: 0x26233A,
    selected: 0x21202E,
    cursor_text: 0xE0DEF4,
    marked: 0xF6C177,
    list_fill: 0x191724,
    list_text: 0xE0DEF4,
    list_text_secondary: 0x908CAA,
    stripe: 0x1C1A29,
    control: 0x26233A,
    syntax: [
        0x5089A0, 0x9CCFD8, 0xF6C177, 0xC4A7E7, 0xEBBCBA, 0x908CAA, 0x908CAA, 0x908CAA, 0x9CCFD8,
        0xEBBCBA, 0xEB6F92,
    ],
    terminal: [
        0xE0DEF4, 0x191724, 0x26233A, 0xEB6F92, 0x31748F, 0xF6C177, 0x9CCFD8, 0xC4A7E7, 0xEBBCBA,
        0xE0DEF4, 0x6E6A86, 0xEB6F92, 0x31748F, 0xF6C177, 0x9CCFD8, 0xC4A7E7, 0xEBBCBA, 0xE0DEF4,
    ],
};
