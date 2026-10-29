//! A setting's value against its row: the default, the check of a new value, the type's name.

use std::path::Path;

use serde_json::Value;

use super::{MAX_TEXT, SettingKind, SettingSpec};

/// The longest font family name a setting takes; real ones are far shorter.
const MAX_FONT_NAME: usize = 128;
/// The most extensions, and the longest one, an extension list takes.
const MAX_EXTENSIONS: usize = 64;
const MAX_EXTENSION: usize = 16;

impl SettingSpec {
    /// The value while nothing (valid) is stored.
    #[must_use]
    pub fn default_value(&self) -> Value {
        match self.kind {
            SettingKind::Bool { default } => Value::Bool(default),
            SettingKind::Int { default, .. } => Value::from(default),
            SettingKind::Choice { default, .. }
            | SettingKind::Id { default }
            | SettingKind::Path { default }
            | SettingKind::Font { default }
            | SettingKind::Extensions { default }
            | SettingKind::Associations { default }
            | SettingKind::Keymap { default }
            | SettingKind::Colour { default }
            | SettingKind::ColourRules { default }
            | SettingKind::Source { default }
            | SettingKind::Proxy { default } => Value::from(default),
        }
    }

    /// `value` when it is valid for this setting; why not otherwise (never the value itself).
    ///
    /// # Errors
    ///
    /// The type, a bound or the format does not fit.
    pub fn check(&self, value: &Value) -> Result<Value, String> {
        match self.kind {
            SettingKind::Bool { .. } => value
                .as_bool()
                .map(Value::Bool)
                .ok_or_else(|| "expected true or false".to_owned()),
            SettingKind::Int { min, max, .. } => match value.as_i64() {
                Some(number) if (min..=max).contains(&number) => Ok(Value::from(number)),
                _ => Err(format!("expected a whole number from {min} to {max}")),
            },
            SettingKind::Choice { choices, .. } => {
                let text = value.as_str().unwrap_or_default();
                if choices.iter().any(|choice| choice.value == text) {
                    Ok(Value::from(text))
                } else {
                    let names: Vec<&str> = choices.iter().map(|choice| choice.value).collect();
                    Err(format!("expected one of {}", names.join(", ")))
                }
            }
            SettingKind::Id { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                let fits = text.len() <= 64
                    && text
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
                if fits {
                    Ok(Value::from(text))
                } else {
                    Err("expected at most 64 letters, digits, '.', '_' or '-'".to_owned())
                }
            }
            SettingKind::Path { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                if text.is_empty() {
                    return Ok(Value::from(""));
                }
                if text.len() > MAX_TEXT || text.chars().any(char::is_control) {
                    return Err("expected a path without control characters".to_owned());
                }
                if Path::new(text).is_absolute() {
                    Ok(Value::from(text))
                } else {
                    Err("expected an absolute path".to_owned())
                }
            }
            SettingKind::Font { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                let fits = text.len() <= MAX_FONT_NAME
                    && text.trim() == text
                    && !text.chars().any(char::is_control);
                if fits {
                    Ok(Value::from(text))
                } else {
                    Err(format!(
                        "expected a font name of at most {MAX_FONT_NAME} bytes, empty for the default"
                    ))
                }
            }
            SettingKind::Extensions { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                extensions(text).map(Value::from).ok_or_else(|| {
                    format!(
                        "expected at most {MAX_EXTENSIONS} extensions such as \"nfo diz\", \
                         letters, digits, '_' or '-', separated by spaces"
                    )
                })
            }
            SettingKind::Associations { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                let pairs = parse_associations(text).ok_or_else(|| {
                    format!(
                        "expected at most {MAX_EXTENSIONS} pairs such as \"csv=<plugin id>\", \
                         an extension and a plugin's id, separated by spaces"
                    )
                })?;
                let joined: Vec<String> = pairs
                    .iter()
                    .map(|(extension, id)| format!("{extension}={id}"))
                    .collect();
                Ok(Value::from(joined.join(" ")))
            }
            SettingKind::Colour { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                super::colour_value(text).map(Value::from).ok_or_else(|| {
                    "expected a colour such as \"#c42b1c\", empty for the theme's own".to_owned()
                })
            }
            // Written as the rule list writes it, so the same rules read as one value.
            SettingKind::ColourRules { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                let rules = (text.len() <= MAX_TEXT)
                    .then(|| super::parse_colour_rules(text))
                    .flatten()
                    .ok_or_else(|| {
                        format!(
                            "expected at most {} rules, one per line, a mask, a tab and a \
                             colour such as \"#c42b1c\", and a dark colour after another tab",
                            super::MAX_COLOUR_RULES
                        )
                    })?;
                Ok(Value::from(super::write_colour_rules(&rules)))
            }
            SettingKind::Source { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                if text.is_empty() {
                    return Ok(Value::from(""));
                }
                if text.len() > MAX_TEXT {
                    return Err("expected a shorter address".to_owned());
                }
                rc_update::Sources::mirror(text)
                    .map(|_| Value::from(text.trim()))
                    .map_err(|_| "expected an https:// folder without a login".to_owned())
            }
            SettingKind::Proxy { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                if text.len() > MAX_TEXT {
                    return Err("expected a shorter address".to_owned());
                }
                rc_update::ProxyChoice::parse(text)
                    .map(|_| Value::from(text.trim()))
                    .map_err(|reason| {
                        format!("expected empty, direct or http(s)://host:port: {reason}")
                    })
            }
            // Written as the keymap writes it, so the same deviations read as one value.
            SettingKind::Keymap { .. } => {
                let text = value.as_str().ok_or_else(|| "expected a text".to_owned())?;
                crate::Overrides::parse(text)
                    .map(|overrides| Value::from(overrides.to_string()))
                    .map_err(|_| {
                        format!(
                            "expected at most {} deviations such as \"Ctrl+K=copy F5=\", \
                             a key and a row's name or nothing, none on a key the window keeps",
                            crate::MAX_OVERRIDES
                        )
                    })
            }
        }
    }

    /// The type's name as `settings.get` reports it.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self.kind {
            SettingKind::Bool { .. } => "bool",
            SettingKind::Int { .. } => "int",
            SettingKind::Choice { .. } => "choice",
            SettingKind::Id { .. } => "id",
            SettingKind::Path { .. } => "path",
            SettingKind::Font { .. } => "font",
            SettingKind::Extensions { .. } => "extensions",
            SettingKind::Associations { .. } => "associations",
            SettingKind::Keymap { .. } => "keymap",
            SettingKind::Colour { .. } => "colour",
            SettingKind::ColourRules { .. } => "colour_rules",
            SettingKind::Source { .. } => "source",
            SettingKind::Proxy { .. } => "proxy",
        }
    }
}

/// `text` as an extension list keeps it: lower case, one space between, without the dots or the
/// `*.` a user may type (`*.NFO, .diz` is `nfo diz`); `None` when a word is no extension.
fn extensions(text: &str) -> Option<String> {
    if text.len() > MAX_TEXT {
        return None;
    }
    let words: Vec<String> = text
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';'))
        .map(|word| word.trim_start_matches('*').trim_start_matches('.'))
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let fits = words.len() <= MAX_EXTENSIONS
        && words.iter().all(|word| {
            word.len() <= MAX_EXTENSION
                && word
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
        });
    fits.then(|| words.join(" "))
}

/// `plugins.associations` as pairs of an extension and the plugin id that shows it: `csv=<id>`
/// separated by spaces, the extension written as an extension list keeps it, the id a lower-case
/// UUID; the last pair for an extension wins. `None` when a word is no such pair.
#[must_use]
pub fn parse_associations(text: &str) -> Option<Vec<(String, String)>> {
    if text.len() > MAX_TEXT {
        return None;
    }
    let mut pairs: Vec<(String, String)> = Vec::new();
    for word in text.split_whitespace() {
        let (extension, id) = word.split_once('=')?;
        let extension =
            extensions(extension).filter(|ext| !ext.is_empty() && !ext.contains(' '))?;
        let id = id.to_ascii_lowercase();
        let uuid = id.len() == 36
            && id.chars().enumerate().all(|(at, c)| match at {
                8 | 13 | 18 | 23 => c == '-',
                _ => c.is_ascii_hexdigit(),
            });
        if !uuid {
            return None;
        }
        pairs.retain(|(known, _)| *known != extension);
        pairs.push((extension, id));
    }
    (pairs.len() <= MAX_EXTENSIONS).then_some(pairs)
}
