//! The font rows of the Appearance page (RC-010-17, after Total Commander's Configuration,
//! Fonts): the installed families with the area's default first - monospaced ones only while
//! "Monospace fonts only" (beside the file list's font) is on - a line of sample text in the
//! chosen font and size, and a note when the chosen family is not installed, which then draws in
//! the default.

use std::fmt;

use iced::{
    Alignment, Element, Font,
    widget::{Column, pick_list, row, text},
};
use rc_engine::settings::{
    DEFAULT_FONT_SIZE, LIST_FONT_KEY, LIST_FONT_SIZE_KEY, TERMINAL_FONT_KEY,
    TERMINAL_FONT_SIZE_KEY, VIEWER_FONT_KEY, VIEWER_FONT_SIZE_KEY,
};

use super::{
    SettingsMessage,
    draft::{Draft, Field},
    view::hint,
    wrap,
};
use crate::{
    app::Message,
    i18n::t,
    theme::{
        Tokens, focus,
        fonts::{AreaFont, Family, Fonts, intern, sizes_of},
        layout,
    },
};

/// Each font setting with its size setting and the area's default.
const AREAS: [(&str, &str, AreaFont); 3] = [
    (LIST_FONT_KEY, LIST_FONT_SIZE_KEY, Fonts::DEFAULT.list),
    (
        TERMINAL_FONT_KEY,
        TERMINAL_FONT_SIZE_KEY,
        Fonts::DEFAULT.terminal,
    ),
    (VIEWER_FONT_KEY, VIEWER_FONT_SIZE_KEY, Fonts::DEFAULT.viewer),
];

/// A family in the list; the empty one is the area's default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FontChoice(String);

/// The choices of the font lists for `fonts`: every family, then monospaced ones only - each
/// after the area's default.
pub(super) fn choices(fonts: &[Family]) -> [Vec<FontChoice>; 2] {
    let list = |mono_only: bool| {
        std::iter::once(FontChoice(String::new()))
            .chain(
                fonts
                    .iter()
                    .filter(|family| family.mono || !mono_only)
                    .map(|family| FontChoice(family.name.clone())),
            )
            .collect()
    };
    [list(false), list(true)]
}

impl fmt::Display for FontChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            formatter.write_str(t!("settings.fonts.default"))
        } else {
            formatter.write_str(&self.0)
        }
    }
}

/// The control of the font setting `key`: the list of families, the sample, and the note.
pub(super) fn control<'a>(
    key: &'static str,
    draft: &'a Draft,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let chosen = text_of(draft, key);
    let options = draft.font_choices();
    let current = FontChoice(chosen.to_owned());
    let edit = move |choice: FontChoice| wrap(SettingsMessage::Edit(key, choice.0));
    let moves = focus::moves(options, Some(&current), &edit);
    let picker = pick_list(options, Some(current), edit).width(280);
    let picker = focus::Stop::pick(picker, key, moves).into();
    let mut rows = Column::new().spacing(4);
    rows = rows.push(if key == LIST_FONT_KEY {
        let filter = layout::check(
            t!("settings.fonts.mono_only"),
            draft.mono_only,
            true,
            |on| wrap(SettingsMessage::MonoOnly(on)),
        );
        Element::from(row![picker, filter].spacing(12).align_y(Alignment::Center))
    } else {
        picker
    });
    let installed = draft.fonts().iter().any(|family| family.name == chosen);
    if !chosen.is_empty() && !installed {
        rows = rows.push(hint(t!("settings.fonts.missing"), tokens));
    }
    let Some((_, size_key, default)) = AREAS.iter().find(|(font_key, ..)| *font_key == key) else {
        return rows.into();
    };
    // A family that is not installed draws in the default, as it will in its area.
    let font = match intern_installed(chosen, installed) {
        Some(name) => Font::with_name(name),
        None => default.font,
    };
    let sample = text(t!("settings.fonts.preview"))
        .font(font)
        .size(preview_size(text_of(draft, size_key), size_key));
    rows.push(sample).into()
}

/// The chosen name for `Font::with_name`, only when it is installed.
fn intern_installed(name: &str, installed: bool) -> Option<&'static str> {
    if installed { intern(name) } else { None }
}

/// The size typed into the size field `size_key`, held to its range (8 to 24, the viewer's 6 to
/// 72); the default while it is not a number.
pub(super) fn preview_size(typed: &str, size_key: &str) -> f32 {
    let sizes = sizes_of(size_key);
    typed
        .trim()
        .parse::<i64>()
        .map_or(DEFAULT_FONT_SIZE, |size| {
            size.clamp(*sizes.start(), *sizes.end())
        }) as f32
}

fn text_of<'a>(draft: &'a Draft, key: &str) -> &'a str {
    match draft.field(key) {
        Some(Field::Text(text)) => text.as_str(),
        _ => "",
    }
}

#[cfg(test)]
#[path = "fonts_page_tests.rs"]
mod tests;
