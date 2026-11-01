//! Kanagawa (rebelot, <https://github.com/rebelot/kanagawa.nvim>, MIT, credited in `NOTICE`): Wave
//! for the dark mode and Lotus for the light, each with its own syntax groups and terminal.
//! Syntax colours below 4.5:1 on the surface are a little lighter (dark) or darker (light) than
//! the palette's; so are Lotus's secondary text and marked name.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xDCD5AC,
    surface: 0xF2ECBC,
    header: 0xE5DDB0,
    stroke: 0xD5CEA3,
    text: 0x545464,
    text_secondary: 0x626055,
    accent: 0x4D699B,
    cursor: 0xC9CBD1,
    cursor_inactive: 0xE7DBA0,
    selected: 0xE5DDB0,
    cursor_text: 0x545464,
    marked: 0xA93746,
    list_fill: 0xF2ECBC,
    list_text: 0x545464,
    list_text_secondary: 0x626055,
    stripe: 0xEDE6B7,
    control: 0xE5DDB0,
    syntax: [
        0x624C83, 0x706A3B, 0x5B7040, 0x9E506A, 0x9F5500, 0x6A6963, 0x3E7082, 0x796644, 0x506F69,
        0x4D699B, 0xCC2020,
    ],
    terminal: [
        0x545464, 0xF2ECBC, 0x1F1F28, 0xC84053, 0x6F894E, 0x77713F, 0x4D699B, 0xB35B79, 0x597B75,
        0x545464, 0x8A8980, 0xD7474B, 0x6E915F, 0x836F4A, 0x6693BF, 0x624C83, 0x5E857A, 0x43436C,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x181820,
    surface: 0x1F1F28,
    header: 0x2A2A37,
    stroke: 0x363646,
    text: 0xDCD7BA,
    text_secondary: 0xC8C093,
    accent: 0x7E9CD8,
    cursor: 0x2D4F67,
    cursor_inactive: 0x223249,
    selected: 0x2A2A37,
    cursor_text: 0xDCD7BA,
    marked: 0xE6C384,
    list_fill: 0x1F1F28,
    list_text: 0xDCD7BA,
    list_text_secondary: 0xC8C093,
    stripe: 0x23232E,
    control: 0x2A2A37,
    syntax: [
        0x957FB8, 0xE6C384, 0x98BB6C, 0xD27E99, 0xFFA066, 0x898881, 0x9CABCA, 0xC0A36E, 0x7AA89F,
        0x7E9CD8, 0xED5050,
    ],
    terminal: [
        0xDCD7BA, 0x1F1F28, 0x16161D, 0xC34043, 0x76946A, 0xC0A36E, 0x7E9CD8, 0x957FB8, 0x6A9589,
        0xC8C093, 0x727169, 0xE82424, 0x98BB6C, 0xE6C384, 0x7FB4CA, 0x938AA9, 0x7AA89F, 0xDCD7BA,
    ],
};
