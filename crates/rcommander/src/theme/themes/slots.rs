//! Where each token of a theme (`rc_engine::settings::themes::THEME_TOKENS`) sits in a look: one
//! list, from which the setter, the getter and the key list are written, so the three never part
//! (`slots_tests.rs` holds the keys to the engine's).

use std::collections::BTreeMap;

use iced::Color;

use crate::theme::{Tokens, colours::over};

/// Writes [`slot`], [`value`] and [`KEYS`] from one list of `"key" => field.path`.
macro_rules! slots {
    ($($key:literal => $($field:ident).+,)*) => {
        /// The place of the token `key` in `tokens`; `None` for a key no theme has.
        pub fn slot<'a>(tokens: &'a mut Tokens, key: &str) -> Option<&'a mut Color> {
            match key {
                $($key => Some(&mut tokens.$($field).+),)*
                _ => None,
            }
        }

        /// The colour of the token `key` in `tokens`, as the look holds it (maybe translucent).
        pub fn value(tokens: &Tokens, key: &str) -> Option<Color> {
            match key {
                $($key => Some(tokens.$($field).+),)*
                _ => None,
            }
        }

        /// Every token, in the editor's order.
        pub const KEYS: &[&str] = &[$($key,)*];
    };
}

slots! {
    "window" => window,
    "surface" => surface,
    "header" => header,
    "stroke" => stroke,
    "text" => text,
    "text_secondary" => text_secondary,
    "cursor_fill" => cursor_fill,
    "cursor_fill_inactive" => cursor_fill_inactive,
    "selected_fill" => selected_fill,
    "cursor_text" => cursor_text,
    "marked" => marked,
    "list_fill" => list_fill,
    "list_text" => list_text,
    "list_text_secondary" => list_text_secondary,
    "stripe_fill" => stripe_fill,
    "control" => control,
    "syntax.keyword" => syntax.keyword,
    "syntax.key" => syntax.key,
    "syntax.string" => syntax.string,
    "syntax.number" => syntax.number,
    "syntax.literal" => syntax.literal,
    "syntax.comment" => syntax.comment,
    "syntax.punctuation" => syntax.punctuation,
    "syntax.operator" => syntax.operator,
    "syntax.type_name" => syntax.type_name,
    "syntax.function" => syntax.function,
    "syntax.invalid" => syntax.invalid,
    "terminal.foreground" => terminal.foreground,
    "terminal.background" => terminal.background,
    "terminal.black" => terminal.black,
    "terminal.red" => terminal.red,
    "terminal.green" => terminal.green,
    "terminal.yellow" => terminal.yellow,
    "terminal.blue" => terminal.blue,
    "terminal.magenta" => terminal.magenta,
    "terminal.cyan" => terminal.cyan,
    "terminal.white" => terminal.white,
    "terminal.bright_black" => terminal.bright_black,
    "terminal.bright_red" => terminal.bright_red,
    "terminal.bright_green" => terminal.bright_green,
    "terminal.bright_yellow" => terminal.bright_yellow,
    "terminal.bright_blue" => terminal.bright_blue,
    "terminal.bright_magenta" => terminal.bright_magenta,
    "terminal.bright_cyan" => terminal.bright_cyan,
    "terminal.bright_white" => terminal.bright_white,
}

/// The tokens a look derives from its accent: an own theme that follows the system's accent
/// leaves them out, so they follow it too.
pub const FROM_ACCENT: [&str; 3] = ["cursor_fill", "selected_fill", "marked"];

/// `colour` as `#rrggbb`.
pub fn hex_of(colour: Color) -> String {
    let [red, green, blue, _] = colour.into_rgba8();
    format!("#{red:02x}{green:02x}{blue:02x}")
}

/// The colour of `key` as it is seen: a translucent fill over the lists' fill it is drawn on.
pub fn seen(tokens: &Tokens, key: &str) -> Option<Color> {
    let colour = value(tokens, key)?;
    Some(if colour.a < 1.0 {
        over(colour, tokens.list_fill)
    } else {
        colour
    })
}

/// Every token of `tokens` as an own theme keeps it - `#rrggbb` as it is seen - but those made of
/// the accent when `system_accent`: what Duplicate and the export of a built-in theme write.
pub fn flatten(tokens: &Tokens, system_accent: bool) -> BTreeMap<String, String> {
    KEYS.iter()
        .filter(|key| !(system_accent && FROM_ACCENT.contains(*key)))
        .filter_map(|key| Some(((*key).to_owned(), hex_of(seen(tokens, key)?))))
        .collect()
}

#[cfg(test)]
#[path = "slots_tests.rs"]
mod tests;
