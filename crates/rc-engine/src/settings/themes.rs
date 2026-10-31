//! The colour themes (RC-100-50): named sets of colours, one theme for the light and one for the
//! dark mode (`appearance.theme_light`, `appearance.theme_dark`; `appearance.theme` decides which
//! is in force).
//!
//! A theme sets every token of the window's look but `dark` - window, surfaces, text, accent,
//! cursor, selection, stripes, controls, the lists' and the syntax colours - and the terminal's
//! palette: [`THEME_TOKENS`], each `#rrggbb`. A token a theme lacks (an older theme, a new token)
//! takes the fixed set of its mode. The accent is a colour or the system's ([`ThemeAccent`]).
//!
//! The built-in themes ([`BUILTIN_THEMES`]) are code: their ids, names and modes here, their
//! colours in the window (`crates/rcommander/src/theme/builtin/`); they are neither changed nor
//! deleted - Duplicate makes an own theme of one. Own themes are rows of `rc_db`'s `themes`
//! table, checked here on the way in and on the way back. `theme.list` and `theme.select` are
//! commands (`commands.rs`); the editor, the import and the export of a `.rctheme` file
//! (`file.rs`) are the window's, through `theme.save` and `theme.delete`. The colours of
//! RC-100-43 become an own theme at the start (`adopt.rs`).

mod adopt;
mod commands;
mod file;

use std::collections::BTreeMap;

use rc_db::{Db, StoredTheme};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub use adopt::{CUSTOM_DARK, CUSTOM_LIGHT, adopt_colours};
pub use file::{
    FILE_EXTENSION, FILE_FORMAT, FILE_VERSION, ImportedTheme, MAX_FILE_BYTES, ThemeFileError,
    parse_file, write_file,
};

use super::{THEME_DARK_KEY, THEME_LIGHT_KEY, stored_setting};

/// Every token a theme sets, in the editor's order: the suffix of nothing, the key of a theme's
/// `tokens` object. The window maps each to its place in its look (`theme/themes/slots.rs`).
pub const THEME_TOKENS: [&str; 45] = [
    "window",
    "surface",
    "header",
    "stroke",
    "text",
    "text_secondary",
    "cursor_fill",
    "cursor_fill_inactive",
    "selected_fill",
    "cursor_text",
    "marked",
    "list_fill",
    "list_text",
    "list_text_secondary",
    "stripe_fill",
    "control",
    "syntax.keyword",
    "syntax.key",
    "syntax.string",
    "syntax.number",
    "syntax.literal",
    "syntax.comment",
    "syntax.punctuation",
    "syntax.operator",
    "syntax.type_name",
    "syntax.function",
    "syntax.invalid",
    "terminal.foreground",
    "terminal.background",
    "terminal.black",
    "terminal.red",
    "terminal.green",
    "terminal.yellow",
    "terminal.blue",
    "terminal.magenta",
    "terminal.cyan",
    "terminal.white",
    "terminal.bright_black",
    "terminal.bright_red",
    "terminal.bright_green",
    "terminal.bright_yellow",
    "terminal.bright_blue",
    "terminal.bright_magenta",
    "terminal.bright_cyan",
    "terminal.bright_white",
];

/// The longest name of a theme, in characters.
pub const MAX_THEME_NAME: usize = 64;

/// The prefix of an own theme's id.
pub const OWN_PREFIX: &str = "own-";

/// The theme of each mode while none is chosen.
pub const DEFAULT_LIGHT: &str = "fluent-light";
pub const DEFAULT_DARK: &str = "fluent-dark";

/// A built-in theme: its id, its English name (the window shows it translated) and its mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinTheme {
    pub id: &'static str,
    pub name: &'static str,
    pub dark: bool,
    /// Offered on macOS only: the Mac's own greys (RC-100-49).
    pub macos_only: bool,
}

const fn builtin_row(id: &'static str, name: &'static str, dark: bool) -> BuiltinTheme {
    BuiltinTheme {
        id,
        name,
        dark,
        macos_only: false,
    }
}

/// The built-in themes, light and dark of each, in the order the window lists them.
pub const BUILTIN_THEMES: [BuiltinTheme; 12] = [
    builtin_row(DEFAULT_LIGHT, "Fluent", false),
    builtin_row(DEFAULT_DARK, "Fluent", true),
    BuiltinTheme {
        macos_only: true,
        ..builtin_row("mac-light", "Mac", false)
    },
    BuiltinTheme {
        macos_only: true,
        ..builtin_row("mac-dark", "Mac", true)
    },
    builtin_row("classic-light", "Classic", false),
    builtin_row("classic-dark", "Classic", true),
    builtin_row("high-contrast-light", "High contrast", false),
    builtin_row("high-contrast-dark", "High contrast", true),
    builtin_row("solarized-light", "Solarized", false),
    builtin_row("solarized-dark", "Solarized", true),
    builtin_row("nord-light", "Nord", false),
    builtin_row("nord-dark", "Nord", true),
];

impl BuiltinTheme {
    /// Whether this system offers it.
    #[must_use]
    pub fn available(&self) -> bool {
        !self.macos_only || cfg!(target_os = "macos")
    }
}

/// The built-in theme `id` this system offers.
#[must_use]
pub fn builtin(id: &str) -> Option<&'static BuiltinTheme> {
    BUILTIN_THEMES
        .iter()
        .find(|theme| theme.id == id && theme.available())
}

/// The light or the dark mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    Light,
    Dark,
}

impl ThemeMode {
    #[must_use]
    pub fn of(dark: bool) -> Self {
        if dark { Self::Dark } else { Self::Light }
    }

    #[must_use]
    pub fn is_dark(self) -> bool {
        self == Self::Dark
    }

    /// The setting that names this mode's theme.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Light => THEME_LIGHT_KEY,
            Self::Dark => THEME_DARK_KEY,
        }
    }

    /// The theme of this mode while none is chosen.
    #[must_use]
    pub fn default_id(self) -> &'static str {
        match self {
            Self::Light => DEFAULT_LIGHT,
            Self::Dark => DEFAULT_DARK,
        }
    }
}

/// A theme's accent: the system's (Windows', macOS'), or a colour of its own.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ThemeAccent {
    #[default]
    System,
    Colour(String),
}

impl ThemeAccent {
    /// `system`, or a colour `#rrggbb`; `None` for anything else.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text.eq_ignore_ascii_case("system") {
            return Some(Self::System);
        }
        hex_colour(text).map(Self::Colour)
    }

    /// As a file and a command write it: `system` or `#rrggbb`.
    #[must_use]
    pub fn as_text(&self) -> &str {
        match self {
            Self::System => "system",
            Self::Colour(colour) => colour,
        }
    }

    /// As the `themes` table keeps it: empty for the system's.
    fn stored(&self) -> String {
        match self {
            Self::System => String::new(),
            Self::Colour(colour) => colour.clone(),
        }
    }
}

/// One theme: built in (its colours are the window's) or own.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub id: String,
    /// A built-in's English name; an own one's as the person gave it.
    pub name: String,
    pub dark: bool,
    pub accent: ThemeAccent,
    /// `token -> "#rrggbb"`, every key one of [`THEME_TOKENS`]; empty for a built-in.
    pub tokens: BTreeMap<String, String>,
    pub builtin: bool,
}

impl Theme {
    /// The built-in `theme`.
    #[must_use]
    pub fn of_builtin(theme: &BuiltinTheme) -> Self {
        Self {
            id: theme.id.to_owned(),
            name: theme.name.to_owned(),
            dark: theme.dark,
            accent: ThemeAccent::System,
            tokens: BTreeMap::new(),
            builtin: true,
        }
    }

    /// The row `stored`, checked as anything read back is: an id that is no own theme's is
    /// refused, a token not known or not `#rrggbb` is dropped with a warning (never its value).
    #[must_use]
    pub fn from_stored(stored: StoredTheme) -> Option<Self> {
        if !is_own_id(&stored.id) {
            tracing::warn!(id = %stored.id, "a stored theme has no own id; it is left out");
            return None;
        }
        let object: BTreeMap<String, Value> =
            serde_json::from_str(&stored.tokens).unwrap_or_default();
        let mut tokens = BTreeMap::new();
        for (token, value) in object {
            match value.as_str().and_then(hex_colour) {
                Some(colour) if THEME_TOKENS.contains(&token.as_str()) => {
                    tokens.insert(token, colour);
                }
                _ => tracing::warn!(id = %stored.id, %token, "a stored theme token is dropped"),
            }
        }
        Some(Self {
            id: stored.id,
            name: clean_name(&stored.name),
            dark: stored.dark,
            accent: ThemeAccent::parse(&stored.accent).unwrap_or_default(),
            tokens,
            builtin: false,
        })
    }

    /// As the `themes` table keeps it.
    #[must_use]
    pub fn to_stored(&self) -> StoredTheme {
        StoredTheme {
            id: self.id.clone(),
            name: self.name.clone(),
            dark: self.dark,
            accent: self.accent.stored(),
            tokens: serde_json::to_string(&self.tokens).unwrap_or_else(|_| "{}".to_owned()),
            saved_at: String::new(),
        }
    }
}

/// A theme to save from the window (`theme.save`): a new one without `id`, else the own theme of
/// that id replaced.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeDraft {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub dark: bool,
    /// `system` or `#rrggbb`.
    #[serde(default)]
    pub accent: String,
    /// `token -> "#rrggbb"`, every key one of [`THEME_TOKENS`].
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
}

/// One row of `theme.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeInfo {
    pub id: String,
    pub name: String,
    pub mode: ThemeMode,
    pub builtin: bool,
    /// It is the theme its mode draws with.
    pub selected: bool,
}

/// What the `theme` commands answer: every theme this system offers, and after `theme.save` the
/// id it was saved under.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemesOutcome {
    pub themes: Vec<ThemeInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved: Option<String>,
}

/// The themes and the one chosen for each mode, as the window draws with them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThemesState {
    /// The built-in themes this system offers, then the own ones (light first, by name).
    pub themes: Vec<Theme>,
    /// The id of the light and of the dark theme in force.
    pub chosen: [String; 2],
}

impl ThemesState {
    /// The theme `id`.
    #[must_use]
    pub fn theme(&self, id: &str) -> Option<&Theme> {
        self.themes.iter().find(|theme| theme.id == id)
    }

    /// The theme in force for the light or the dark mode.
    #[must_use]
    pub fn chosen(&self, dark: bool) -> Option<&Theme> {
        self.theme(&self.chosen[usize::from(dark)])
    }

    /// `theme.list`'s rows.
    #[must_use]
    pub fn infos(&self) -> Vec<ThemeInfo> {
        self.themes
            .iter()
            .map(|theme| ThemeInfo {
                id: theme.id.clone(),
                name: theme.name.clone(),
                mode: ThemeMode::of(theme.dark),
                builtin: theme.builtin,
                selected: self.chosen[usize::from(theme.dark)] == theme.id,
            })
            .collect()
    }
}

/// Every theme and the chosen ones, read from `db`. A setting that names no theme of its mode
/// counts as the mode's default, with a warning: the window then draws with Fluent.
pub async fn load(db: &Db) -> ThemesState {
    let mut themes: Vec<Theme> = BUILTIN_THEMES
        .iter()
        .filter(|theme| theme.available())
        .map(Theme::of_builtin)
        .collect();
    match db.themes().await {
        Ok(stored) => themes.extend(stored.into_iter().filter_map(Theme::from_stored)),
        Err(error) => {
            tracing::warn!(%error, "the own themes cannot be read; the built-in ones apply")
        }
    }
    let mut chosen = [String::new(), String::new()];
    for (slot, mode) in chosen.iter_mut().zip([ThemeMode::Light, ThemeMode::Dark]) {
        let value = stored_setting(db, mode.key()).await;
        let named = value.as_ref().and_then(Value::as_str).unwrap_or_default();
        let fits = themes
            .iter()
            .any(|theme| theme.id == named && theme.dark == mode.is_dark());
        *slot = if fits {
            named.to_owned()
        } else {
            if !named.is_empty() && named != mode.default_id() {
                tracing::warn!(
                    key = mode.key(),
                    "the chosen theme is unknown; Fluent applies"
                );
            }
            mode.default_id().to_owned()
        };
    }
    ThemesState { themes, chosen }
}

/// Whether `id` is an own theme's: `own-` and lower-case letters, digits and `-`.
#[must_use]
pub fn is_own_id(id: &str) -> bool {
    id.strip_prefix(OWN_PREFIX).is_some_and(|rest| {
        !rest.is_empty()
            && id.len() <= 64
            && rest
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    })
}

/// A new own theme's id.
#[must_use]
pub fn new_own_id() -> String {
    format!("{OWN_PREFIX}{:08x}", rand::random::<u32>())
}

/// `name` as a theme keeps it: without control characters, trimmed, at most
/// [`MAX_THEME_NAME`] characters; empty when nothing is left.
#[must_use]
pub fn clean_name(name: &str) -> String {
    let kept: String = name.chars().filter(|c| !c.is_control()).collect();
    kept.trim()
        .chars()
        .take(MAX_THEME_NAME)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

/// `text` as a theme's colour: `#rrggbb` in lower case; `None` for anything else.
#[must_use]
pub fn hex_colour(text: &str) -> Option<String> {
    let digits = text.trim().strip_prefix('#')?;
    (digits.len() == 6 && digits.bytes().all(|byte| byte.is_ascii_hexdigit()))
        .then(|| format!("#{}", digits.to_ascii_lowercase()))
}

/// `tokens` as a theme keeps them, and the names of those it does not know (dropped); `Err` with
/// the first known token whose colour is no `#rrggbb`.
///
/// # Errors
///
/// A known token's value is no `#rrggbb`.
pub fn clean_tokens<'a>(
    tokens: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Result<(BTreeMap<String, String>, Vec<String>), String> {
    let mut kept = BTreeMap::new();
    let mut unknown = Vec::new();
    for (token, value) in tokens {
        if !THEME_TOKENS.contains(&token) {
            unknown.push(token.to_owned());
            continue;
        }
        let colour = hex_colour(value).ok_or_else(|| token.to_owned())?;
        kept.insert(token.to_owned(), colour);
    }
    Ok((kept, unknown))
}

/// `name`, or with ` (2)`, ` (3)`, ... after it, so no theme of `taken` has it (case does not
/// count).
#[must_use]
pub fn unique_name<'a>(name: &str, taken: impl Iterator<Item = &'a str> + Clone) -> String {
    let free = |candidate: &str| {
        !taken
            .clone()
            .any(|other| other.to_lowercase() == candidate.to_lowercase())
    };
    if free(name) {
        return name.to_owned();
    }
    (2..)
        .map(|number| {
            let suffix = format!(" ({number})");
            let room = MAX_THEME_NAME.saturating_sub(suffix.chars().count());
            let stem: String = name.chars().take(room).collect();
            format!("{}{suffix}", stem.trim_end())
        })
        .find(|candidate| free(candidate))
        .unwrap_or_else(|| name.to_owned())
}

#[cfg(test)]
#[path = "themes_tests.rs"]
mod tests;
