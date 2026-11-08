//! The symbol collection's font (RC-110-10): a second, larger subset of Fluent UI System Icons
//! (MIT, `NOTICE`), `resources/icons/rcommander-symbols.ttf`, handed to iced with the app's
//! icon font when the window opens. An own command or a bar button that names a symbol
//! (`rc-symbol:<name>`, `rc_engine::user_commands::symbols`) draws it as this font's glyph, in
//! the text colour of either theme - monochrome as the glyphs of rCommander's own commands. The
//! app's icon font stays small; this one is read only where a symbol shows.
//!
//! `scripts/icon-font.sh --symbols` writes the font and the engine's table; the tests hold the
//! two together.

use iced::Font;
use rc_engine::user_commands::symbols;

/// The font, loaded with the window (`app::startup`).
pub const FONT_BYTES: &[u8] = include_bytes!("../../resources/icons/rcommander-symbols.ttf");
/// The font's family; the settings' font lists leave it out.
pub const FAMILY: &str = "rCommander Symbols";
/// The symbols' font.
pub const FONT: Font = Font::with_name(FAMILY);

/// The glyph `icon_path` names; `None` for a file, nothing, or a name the collection does not
/// have.
pub fn glyph_of(icon_path: &str) -> Option<char> {
    symbols::of_path(icon_path).map(|symbol| symbol.glyph)
}

#[cfg(test)]
#[path = "symbols_tests.rs"]
mod tests;
