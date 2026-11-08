//! The Themes tab's section "WSL distributions" (RC-100-56): each distribution the drive bar
//! found, with a pick list of the light and one of the dark mode's themes, Default first. A pick
//! edits the draft's `appearance.wsl_themes` like any setting, and Save stores it. The section is
//! shown on Windows only ([`SHOWN`]), where WSL is; what a pick writes is worked out here on
//! every platform and tested there.
//!
//! A theme the window no longer offers for its mode (deleted, or of the other mode) shows as
//! Default, and the next pick stores none for it, as a connection's form does (RC-100-51).

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{Column, column, pick_list, row},
};
use rc_engine::settings::{
    WSL_THEMES_KEY, WslTheme, parse_wsl_themes,
    themes::{Theme, ThemesState},
    write_wsl_themes, wsl_theme,
};

use super::actions::shown_name;
use crate::{
    app::{
        Message,
        settings::{
            SettingsMessage,
            draft::{Draft, Field},
            view::{hint, label},
            wrap,
        },
    },
    i18n::t,
    theme::{TEXT_SIZE, Tokens, ellipsis::middle_ellipsis, focus},
};

/// Whether the Themes tab shows the section: where WSL is.
pub(super) const SHOWN: bool = cfg!(windows);

/// The width of a distribution's name before its pick lists.
const NAME_WIDTH: f32 = 160.0;
/// The width of a pick list.
const PICK_WIDTH: f32 = 220.0;

/// A theme in a pick list: its id, `None` for Default, shown by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pick {
    id: Option<String>,
    name: String,
}

impl fmt::Display for Pick {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.name)
    }
}

/// Whether the window offers the theme `id` for the light or the `dark` mode.
fn offered(state: &ThemesState, id: &str, dark: bool) -> bool {
    state.theme(id).is_some_and(|theme| theme.dark == dark)
}

/// The distributions' themes in `text`, without one the window does not offer for its mode.
pub(super) fn known_rows(text: &str, state: &ThemesState) -> Vec<WslTheme> {
    let mut rows = parse_wsl_themes(text).unwrap_or_default();
    for row in &mut rows {
        for (slot, dark) in [(&mut row.light, false), (&mut row.dark, true)] {
            if slot.as_deref().is_some_and(|id| !offered(state, id, dark)) {
                *slot = None;
            }
        }
    }
    rows
}

/// The distributions' themes the draft holds, without one the window does not offer for its mode.
pub(super) fn drafted_rows(draft: &Draft, state: &ThemesState) -> Vec<WslTheme> {
    let text = match draft.field(WSL_THEMES_KEY) {
        Some(Field::Text(text)) => text.as_str(),
        _ => "",
    };
    known_rows(text, state)
}

/// `rows` with the theme of the light or the `dark` mode of `distro` set to `id` (`None`: the
/// window's), written as the setting keeps them; the name compares without regard to case.
pub(super) fn with_theme(
    mut rows: Vec<WslTheme>,
    distro: &str,
    dark: bool,
    id: Option<String>,
) -> String {
    let found = rows
        .iter()
        .position(|row| row.distro.eq_ignore_ascii_case(distro));
    let at = found.unwrap_or_else(|| {
        rows.push(WslTheme {
            distro: distro.to_owned(),
            light: None,
            dark: None,
        });
        rows.len() - 1
    });
    let row = &mut rows[at];
    if dark {
        row.dark = id;
    } else {
        row.light = id;
    }
    write_wsl_themes(&rows)
}

/// The pick list of `distro`'s theme of the light or the `dark` mode.
fn pick<'a>(rows: &[WslTheme], themes: &[Theme], distro: &str, dark: bool) -> Element<'a, Message> {
    let default = Pick {
        id: None,
        name: t!("connections.theme_default").to_owned(),
    };
    let options: Vec<Pick> = std::iter::once(default)
        .chain(
            themes
                .iter()
                .filter(|theme| theme.dark == dark)
                .map(|theme| Pick {
                    id: Some(theme.id.clone()),
                    name: shown_name(theme).to_owned(),
                }),
        )
        .collect();
    let current = wsl_theme(rows, distro).and_then(|row| row.theme(dark));
    let selected = options
        .iter()
        .find(|option| option.id.as_deref() == current)
        .cloned();
    let (rows, owned) = (rows.to_vec(), distro.to_owned());
    let edit = move |picked: Pick| {
        let text = with_theme(rows.clone(), &owned, dark, picked.id);
        wrap(SettingsMessage::Edit(WSL_THEMES_KEY, text))
    };
    let mode = if dark { "dark" } else { "light" };
    let stop = format!("settings.themes.wsl.{distro}.{mode}");
    let moves = focus::moves(&options, selected.as_ref(), &edit);
    let list = pick_list(options, selected, edit).width(PICK_WIDTH);
    focus::Stop::pick(list, &stop, moves).into()
}

/// The section: a line per distribution `distros` names, its name and its two pick lists; a hint
/// where none was found.
pub(super) fn section<'a>(
    distros: &[String],
    state: &ThemesState,
    draft: &Draft,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let rows = drafted_rows(draft, state);
    let mut lines = Column::new().spacing(6).push(
        row![
            label("").width(NAME_WIDTH),
            hint(t!("settings.themes.light"), tokens).width(PICK_WIDTH),
            hint(t!("settings.themes.dark"), tokens).width(PICK_WIDTH),
        ]
        .spacing(8),
    );
    for distro in distros {
        let name = middle_ellipsis(distro, NAME_WIDTH, TEXT_SIZE).into_owned();
        lines = lines.push(
            row![
                label(name).width(NAME_WIDTH),
                pick(&rows, &state.themes, distro, false),
                pick(&rows, &state.themes, distro, true),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
    }
    let body: Element<'a, Message> = if distros.is_empty() {
        hint(t!("settings.themes.wsl_none"), tokens).into()
    } else {
        lines.width(Length::Shrink).into()
    };
    column![
        label(t!("settings.themes.wsl")),
        body,
        hint(t!("settings.themes.wsl_help"), tokens),
    ]
    .spacing(6)
    .into()
}

#[cfg(test)]
#[path = "wsl_tests.rs"]
mod tests;
