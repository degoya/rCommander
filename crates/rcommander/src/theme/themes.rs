//! The looks of the colour themes (RC-100-50): a theme of the engine
//! (`rc_engine::settings::themes`) made into [`Tokens`], and the looks the window draws with.
//!
//! A built-in theme is code (`builtin.rs`); an own one is the fixed set of its mode (`light.rs`,
//! `dark.rs`) with its accent - the system's or its own, which the cursor and the marks are made
//! of - and its tokens laid over it, a token it lacks keeping the fixed set's. A look is built
//! once and kept for the rest of the run like every look (`&'static`): a theme chosen again, or a
//! pause of the editor on colours seen before, takes the look made then ([`look`]). Only looks
//! chosen, asked for ([`tokens_of`], a connection's panels, `connections.rs`, RC-100-51) or
//! edited are built - the editor asks for one per pause, never per key
//! (`app/settings/themes_page/editor.rs`).
//!
//! While the editor is open its theme is the preview: [`tokens`](super::tokens) draws its mode
//! with it, the main window included; Cancel takes it away and the chosen look is back. The
//! active workspace's theme of a mode comes before the chosen one (RC-100-68, `workspace.rs`).

mod connections;
mod slots;
mod workspace;

use std::sync::{
    Mutex, RwLock,
    atomic::{AtomicU64, Ordering},
};

use iced::Color;
use rc_engine::settings::themes::{Theme, ThemeAccent, ThemesState};

pub use connections::{Assigned, of_connection, of_distro, of_path, set_assigned};
pub use slots::{KEYS, flatten, hex_of, seen, slot};

use super::{Tokens, builtin, colours::parse_hex, dark, light};

use crate::recover::LockOrRecover as _;
use crate::recover::RwLockOrRecover as _;

/// The look of `theme`, with `system` the system's accent for its mode.
#[must_use]
pub fn build(theme: &Theme, system: Color) -> Tokens {
    if theme.builtin
        && let Some(tokens) = builtin::look(&theme.id, system)
    {
        return tokens;
    }
    let accent = match &theme.accent {
        ThemeAccent::System => system,
        ThemeAccent::Colour(colour) => parse_hex(colour).unwrap_or(system),
    };
    let mut tokens = if theme.dark {
        dark::tokens(accent)
    } else {
        light::tokens(accent)
    };
    for (key, colour) in &theme.tokens {
        if let (Some(place), Some(colour)) = (slot(&mut tokens, key), parse_hex(colour)) {
            *place = colour;
        }
    }
    tokens
}

/// Every look made in this run, with the theme and the accent it was made of.
static MADE: Mutex<Vec<(Theme, Color, &'static Tokens)>> = Mutex::new(Vec::new());

/// The look of `theme` with the system's accent: the one made before for them, or a new one kept
/// for the rest of the run.
pub fn look(theme: &Theme) -> &'static Tokens {
    let system = super::system_accent()[usize::from(theme.dark)];
    let mut made = MADE.lock_or_recover("themes.made");
    if let Some((.., tokens)) = made
        .iter()
        .find(|(was, accent, _)| was == theme && *accent == system)
    {
        return tokens;
    }
    let tokens: &'static Tokens = Box::leak(Box::new(build(theme, system)));
    made.push((theme.clone(), system, tokens));
    tokens
}

/// What the window draws with: the themes it knows, the looks chosen for both modes, and the
/// editor's preview.
struct Drawn {
    themes: Vec<Theme>,
    chosen: [&'static Tokens; 2],
    /// The ids of the chosen themes, light and dark (the command palette marks them, RC-100-55).
    chosen_ids: [Option<String>; 2],
}

static DRAWN: RwLock<Option<Drawn>> = RwLock::new(None);
/// The editor's preview of the light and of the dark mode, each with the editor it is of.
static PREVIEW: RwLock<[Option<(u64, &'static Tokens)>; 2]> = RwLock::new([None, None]);
/// The last editor's number.
static EDITORS: AtomicU64 = AtomicU64::new(0);

/// The themes and the chosen ones from now on; their looks are built here, once each.
pub fn set_state(state: &ThemesState) {
    let themes = [false, true].map(|dark| {
        let fallback = || {
            let id = if dark { "fluent-dark" } else { "fluent-light" };
            state.theme(id).cloned()
        };
        state.chosen(dark).cloned().or_else(fallback)
    });
    let drawn = Drawn {
        themes: state.themes.clone(),
        chosen: [0, 1].map(|at| match &themes[at] {
            Some(theme) => look(theme),
            None => super::fixed(at == 1),
        }),
        chosen_ids: themes.map(|theme| theme.map(|theme| theme.id)),
    };
    *DRAWN.write_or_recover("themes.drawn") = Some(drawn);
    // An active workspace's theme may have changed with them (RC-100-68).
    workspace::resolve(tokens_of);
}

/// The active workspace's themes from now on, light first, by id (RC-100-68): their looks come
/// before the chosen themes'; whether they changed.
pub fn set_workspace_themes(themes: [Option<String>; 2]) -> bool {
    let changed = workspace::set_themes(themes);
    if changed {
        workspace::resolve(tokens_of);
    }
    changed
}

/// The look the window draws the light or the dark mode with: the editor's preview of that mode,
/// else the active workspace's theme of it (RC-100-68), else the chosen theme's; `None` before
/// the themes are read.
pub fn current(dark: bool) -> Option<&'static Tokens> {
    let preview = PREVIEW.read_or_recover("themes.preview")[usize::from(dark)];
    if let Some((_, preview)) = preview {
        return Some(preview);
    }
    if let Some(tokens) = workspace::look(dark) {
        return Some(tokens);
    }
    DRAWN
        .read_or_recover("themes.drawn")
        .as_ref()
        .map(|drawn| drawn.chosen[usize::from(dark)])
}

/// A number for an editor's previews, its own: an editor ends only its own preview.
pub fn preview_owner() -> u64 {
    EDITORS.fetch_add(1, Ordering::Relaxed) + 1
}

/// The theme the editor `owner` edits, as the preview of its mode from now on.
pub fn set_preview(owner: u64, theme: &Theme) {
    let preview = look(theme);
    let mut previews = PREVIEW.write_or_recover("themes.preview");
    // A theme is of one mode: an editor's preview of the other ends.
    for slot in previews.iter_mut() {
        if slot.is_some_and(|(drawn, _)| drawn == owner) {
            *slot = None;
        }
    }
    previews[usize::from(theme.dark)] = Some((owner, preview));
}

/// The editor `owner` closed: its preview, if it is the one drawn, is taken away.
pub fn end_preview(owner: u64) {
    let mut previews = PREVIEW.write_or_recover("themes.preview");
    for slot in previews.iter_mut() {
        if slot.is_some_and(|(drawn, _)| drawn == owner) {
            *slot = None;
        }
    }
}

/// The look of the theme `id` of the light or the `dark` mode, for a part of the window that
/// draws with a theme of its own (a connection's panel, RC-100-51); `None` for an id the window
/// does not know or a theme of the other mode.
pub fn tokens_of(id: &str, dark: bool) -> Option<&'static Tokens> {
    let theme = DRAWN
        .read_or_recover("themes.drawn")
        .as_ref()?
        .themes
        .iter()
        .find(|theme| theme.id == id && theme.dark == dark)?
        .clone();
    Some(look(&theme))
}

/// The id of the theme the light or the `dark` mode draws with; `None` before the themes are
/// read.
pub fn chosen_id(dark: bool) -> Option<String> {
    DRAWN
        .read_or_recover("themes.drawn")
        .as_ref()
        .and_then(|drawn| drawn.chosen_ids[usize::from(dark)].clone())
}

/// The themes of the light or the `dark` mode the window knows, built-in ones first: what a
/// connection can choose (RC-100-51).
pub fn of_mode(dark: bool) -> Vec<Theme> {
    DRAWN
        .read_or_recover("themes.drawn")
        .as_ref()
        .map(|drawn| {
            drawn
                .themes
                .iter()
                .filter(|theme| theme.dark == dark)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "themes_tests.rs"]
mod tests;
