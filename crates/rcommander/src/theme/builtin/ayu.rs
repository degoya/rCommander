//! Ayu (ayu-theme, <https://github.com/ayu-theme/ayu-colors>, MIT, credited in `NOTICE`): Ayu Dark
//! and Ayu Light as `ayu` 9.1.0 generates them - editor, panels, syntax and terminal; the
//! cursor rows are the editor's selection laid over the list. Syntax colours below 4.5:1 on the
//! surface are a little lighter (dark) or darker (light) than the palette's, as are the secondary
//! text and Ayu Light's marked name; Ayu Dark's bright black is the comment grey instead of
//! the near-black that ships, which vanishes on the background.

use super::Palette;

pub(in crate::theme) const LIGHT: Palette = Palette {
    dark: false,
    window: 0xEBEEF0,
    surface: 0xFCFCFC,
    header: 0xF8F9FA,
    stroke: 0xEAEDEF,
    text: 0x5C6166,
    text_secondary: 0x656D7A,
    accent: 0xF29718,
    cursor: 0xD7E4F6,
    cursor_inactive: 0xE8EAED,
    selected: 0xF0F1F3,
    cursor_text: 0x5C6166,
    marked: 0xBE4242,
    list_fill: 0xFCFCFC,
    list_text: 0x5C6166,
    list_text_secondary: 0x656D7A,
    stripe: 0xF8F9FA,
    control: 0xF8F9FA,
    syntax: [
        0xAF5D23, 0x3B7C92, 0x5E7D00, 0x8765A9, 0x9E6830, 0x727375, 0x5C6166, 0x9B675D, 0x1A7BAC,
        0x996B00, 0xCA4646,
    ],
    terminal: [
        0x5C6166, 0xFCFCFC, 0x86878C, 0xF07171, 0x86B300, 0xEBA400, 0x22A4E6, 0xA37ACC, 0x4CBF99,
        0xADAEB1, 0x939498, 0xF07171, 0x86B300, 0xEBA400, 0x22A4E6, 0xA37ACC, 0x4CBF99, 0xC5C5C8,
    ],
};

pub(in crate::theme) const DARK: Palette = Palette {
    dark: true,
    window: 0x0D1017,
    surface: 0x10141C,
    header: 0x141821,
    stroke: 0x1B1F29,
    text: 0xBFBDB6,
    text_secondary: 0x7C8393,
    accent: 0xE6B450,
    cursor: 0x193155,
    cursor_inactive: 0x1E242F,
    selected: 0x161A24,
    cursor_text: 0xBFBDB6,
    marked: 0xE6B450,
    list_fill: 0x10141C,
    list_text: 0xBFBDB6,
    list_text_secondary: 0x7C8393,
    stripe: 0x131720,
    control: 0x141821,
    syntax: [
        0xFF8F40, 0x39BAE6, 0xAAD94C, 0xD2A6FF, 0xE6C08A, 0x76808B, 0xBFBDB6, 0xF29668, 0x59C2FF,
        0xFFB454, 0xD95757,
    ],
    terminal: [
        0xBFBDB6, 0x10141C, 0x0A0000, 0xE6495A, 0x97C142, 0xE89D37, 0x17ACF2, 0xC385FE, 0x84CEB5,
        0xFFFFFF, 0x5A6673, 0xF07178, 0xAAD94C, 0xFFB454, 0x59C2FF, 0xD2A6FF, 0x95E6CB, 0xFFFFFF,
    ],
};
