//! The colours of a plugin document's coloured text (RC-100-10, `docs/design.md` "Colour"): one
//! per token class, in the light and the dark look. A plugin names a class, never a colour, so
//! it can neither make text unreadable nor look like rCommander's own controls. The colours are
//! Visual Studio Code's Light+ and Dark+, held to 4.5:1 against `surface` (`theme_tests.rs`).

use iced::Color;

use super::hex;

/// One colour per token class.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Syntax {
    pub keyword: Color,
    pub key: Color,
    pub string: Color,
    pub number: Color,
    pub literal: Color,
    pub comment: Color,
    pub punctuation: Color,
    pub operator: Color,
    pub type_name: Color,
    pub function: Color,
    pub invalid: Color,
}

/// On `#FFFFFF`.
pub(super) const LIGHT: Syntax = Syntax {
    keyword: hex(0x0000FF, 1.0),
    key: hex(0x0451A5, 1.0),
    string: hex(0xA31515, 1.0),
    number: hex(0x098658, 1.0),
    literal: hex(0xAF00DB, 1.0),
    comment: hex(0x008000, 1.0),
    punctuation: hex(0x616161, 1.0),
    operator: hex(0x1A1A1A, 1.0),
    type_name: hex(0x267F99, 1.0),
    function: hex(0x795E26, 1.0),
    invalid: hex(0xCD3131, 1.0),
};

/// On `#272727`; the comment and the invalid red a little lighter than Dark+'s for the contrast.
pub(super) const DARK: Syntax = Syntax {
    keyword: hex(0x569CD6, 1.0),
    key: hex(0x9CDCFE, 1.0),
    string: hex(0xCE9178, 1.0),
    number: hex(0xB5CEA8, 1.0),
    literal: hex(0xC586C0, 1.0),
    comment: hex(0x73A85E, 1.0),
    punctuation: hex(0xC5C5C5, 1.0),
    operator: hex(0xD4D4D4, 1.0),
    type_name: hex(0x4EC9B0, 1.0),
    function: hex(0xDCDCAA, 1.0),
    invalid: hex(0xF36A6A, 1.0),
};
