//! The colours of the file lists per theme and the colours by file type (RC-100-43, after Total
//! Commander's Configuration, Color): eight colours each for the light and the dark look, every
//! one `#rrggbb` or empty for the theme's own, and an ordered list of rules "mask -> colour" that
//! colour a name by the first rule it matches.
//!
//! A rule is one line `mask<TAB>#light[<TAB>#dark]`, the rules joined by line breaks. The mask is
//! Total Commander's selection mask (`*.exe;*.bat`, `*\` for folders, `|` before exclusions), the
//! dark colour falls back to the light one. Neither is protected: a colour changes what the
//! person sees, never what is worked on.

use super::{SettingKind, SettingPage, SettingSpec};

/// The rules by file type.
pub const COLOUR_RULES_KEY: &str = "appearance.colour_rules";
/// The most rules, and the longest mask, a rule list takes.
pub const MAX_COLOUR_RULES: usize = 64;
const MAX_MASK: usize = 256;

/// The colours one theme can override, in the dialog's order: the suffix of each key.
pub const COLOUR_TOKENS: [&str; 8] = [
    "text",
    "background",
    "stripe",
    "cursor",
    "cursor_inactive",
    "cursor_text",
    "marked",
    "accent",
];

/// The key of colour `token` (one of [`COLOUR_TOKENS`]) in the light or the dark look.
#[must_use]
pub fn colour_key(dark: bool, token: &str) -> Option<&'static str> {
    let keys = if dark { &DARK_KEYS } else { &LIGHT_KEYS };
    let index = COLOUR_TOKENS.iter().position(|known| *known == token)?;
    Some(keys[index])
}

const LIGHT_KEYS: [&str; 8] = [
    LIGHT_TEXT.key,
    LIGHT_BACKGROUND.key,
    LIGHT_STRIPE.key,
    LIGHT_CURSOR.key,
    LIGHT_CURSOR_INACTIVE.key,
    LIGHT_CURSOR_TEXT.key,
    LIGHT_MARKED.key,
    LIGHT_ACCENT.key,
];

const DARK_KEYS: [&str; 8] = [
    DARK_TEXT.key,
    DARK_BACKGROUND.key,
    DARK_STRIPE.key,
    DARK_CURSOR.key,
    DARK_CURSOR_INACTIVE.key,
    DARK_CURSOR_TEXT.key,
    DARK_MARKED.key,
    DARK_ACCENT.key,
];

/// Every colour key and the rule list: what the window reads when one of them changes.
pub const COLOUR_KEYS: [&str; 17] = [
    LIGHT_KEYS[0],
    LIGHT_KEYS[1],
    LIGHT_KEYS[2],
    LIGHT_KEYS[3],
    LIGHT_KEYS[4],
    LIGHT_KEYS[5],
    LIGHT_KEYS[6],
    LIGHT_KEYS[7],
    DARK_KEYS[0],
    DARK_KEYS[1],
    DARK_KEYS[2],
    DARK_KEYS[3],
    DARK_KEYS[4],
    DARK_KEYS[5],
    DARK_KEYS[6],
    DARK_KEYS[7],
    COLOUR_RULES_KEY,
];

const fn colour(key: &'static str, label: &'static str, help: &'static str) -> SettingSpec {
    SettingSpec {
        key,
        page: SettingPage::Appearance,
        kind: SettingKind::Colour { default: "" },
        label,
        help,
        protected: false,
        restart: false,
    }
}

// The rows, light first, then dark, then the rules: one constant each, as every area module
// writes its rows.
pub(super) const LIGHT_TEXT: SettingSpec = colour(
    "appearance.colours.light.text",
    "setting.colours.light.text",
    "setting.colours.light.text.help",
);
pub(super) const LIGHT_BACKGROUND: SettingSpec = colour(
    "appearance.colours.light.background",
    "setting.colours.light.background",
    "setting.colours.light.background.help",
);
pub(super) const LIGHT_STRIPE: SettingSpec = colour(
    "appearance.colours.light.stripe",
    "setting.colours.light.stripe",
    "setting.colours.light.stripe.help",
);
pub(super) const LIGHT_CURSOR: SettingSpec = colour(
    "appearance.colours.light.cursor",
    "setting.colours.light.cursor",
    "setting.colours.light.cursor.help",
);
pub(super) const LIGHT_CURSOR_INACTIVE: SettingSpec = colour(
    "appearance.colours.light.cursor_inactive",
    "setting.colours.light.cursor_inactive",
    "setting.colours.light.cursor_inactive.help",
);
pub(super) const LIGHT_CURSOR_TEXT: SettingSpec = colour(
    "appearance.colours.light.cursor_text",
    "setting.colours.light.cursor_text",
    "setting.colours.light.cursor_text.help",
);
pub(super) const LIGHT_MARKED: SettingSpec = colour(
    "appearance.colours.light.marked",
    "setting.colours.light.marked",
    "setting.colours.light.marked.help",
);
pub(super) const LIGHT_ACCENT: SettingSpec = colour(
    "appearance.colours.light.accent",
    "setting.colours.light.accent",
    "setting.colours.light.accent.help",
);
pub(super) const DARK_TEXT: SettingSpec = colour(
    "appearance.colours.dark.text",
    "setting.colours.dark.text",
    "setting.colours.dark.text.help",
);
pub(super) const DARK_BACKGROUND: SettingSpec = colour(
    "appearance.colours.dark.background",
    "setting.colours.dark.background",
    "setting.colours.dark.background.help",
);
pub(super) const DARK_STRIPE: SettingSpec = colour(
    "appearance.colours.dark.stripe",
    "setting.colours.dark.stripe",
    "setting.colours.dark.stripe.help",
);
pub(super) const DARK_CURSOR: SettingSpec = colour(
    "appearance.colours.dark.cursor",
    "setting.colours.dark.cursor",
    "setting.colours.dark.cursor.help",
);
pub(super) const DARK_CURSOR_INACTIVE: SettingSpec = colour(
    "appearance.colours.dark.cursor_inactive",
    "setting.colours.dark.cursor_inactive",
    "setting.colours.dark.cursor_inactive.help",
);
pub(super) const DARK_CURSOR_TEXT: SettingSpec = colour(
    "appearance.colours.dark.cursor_text",
    "setting.colours.dark.cursor_text",
    "setting.colours.dark.cursor_text.help",
);
pub(super) const DARK_MARKED: SettingSpec = colour(
    "appearance.colours.dark.marked",
    "setting.colours.dark.marked",
    "setting.colours.dark.marked.help",
);
pub(super) const DARK_ACCENT: SettingSpec = colour(
    "appearance.colours.dark.accent",
    "setting.colours.dark.accent",
    "setting.colours.dark.accent.help",
);
pub(super) const RULES: SettingSpec = SettingSpec {
    key: COLOUR_RULES_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::ColourRules { default: "" },
    label: "setting.colours.rules",
    help: "setting.colours.rules.help",
    protected: false,
    restart: false,
};

/// `text` as a colour setting keeps it: `#rrggbb` in lower case (`#RGB` widened), or empty for
/// the theme's own; `None` for anything else.
#[must_use]
pub fn colour_value(text: &str) -> Option<String> {
    let text = text.trim();
    if text.is_empty() {
        return Some(String::new());
    }
    let digits = text.strip_prefix('#')?;
    if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let wide: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 => digits.to_owned(),
        _ => return None,
    };
    Some(format!("#{}", wide.to_ascii_lowercase()))
}

/// One rule as stored: its mask, its colour in the light look and, when it has one of its own,
/// in the dark look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColourRule {
    pub mask: String,
    pub light: String,
    pub dark: Option<String>,
}

/// The rules of `text`, each colour as [`colour_value`] keeps it; `None` when a line has no
/// mask, a colour that does not read, or there are too many.
#[must_use]
pub fn parse_colour_rules(text: &str) -> Option<Vec<ColourRule>> {
    let rules: Vec<ColourRule> = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.split('\t');
            let mask = fields.next()?.trim();
            let fits =
                !mask.is_empty() && mask.len() <= MAX_MASK && !mask.chars().any(char::is_control);
            let light = colour_value(fields.next()?).filter(|light| !light.is_empty())?;
            let dark = match fields.next() {
                Some(dark) => colour_value(dark)?,
                None => String::new(),
            };
            (fits && fields.next().is_none()).then(|| ColourRule {
                mask: mask.to_owned(),
                light,
                dark: (!dark.is_empty()).then_some(dark),
            })
        })
        .collect::<Option<_>>()?;
    (rules.len() <= MAX_COLOUR_RULES).then_some(rules)
}

/// `rules` written as a rule list: one line each.
#[must_use]
pub fn write_colour_rules(rules: &[ColourRule]) -> String {
    rules
        .iter()
        .map(|rule| match &rule.dark {
            Some(dark) => format!("{}\t{}\t{dark}", rule.mask, rule.light),
            None => format!("{}\t{}", rule.mask, rule.light),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[path = "colours_tests.rs"]
mod tests;
