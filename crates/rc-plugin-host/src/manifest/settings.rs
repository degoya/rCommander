//! `[[settings]]`: what a plugin's own settings may be - a choice among the values its manifest
//! lists, or free text held to the host's rule.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/manifest/settings.rs` at
//! commit 26c8592a6 (RD-170-09, RD-1250-01): the same limits and the same rule for free text (at
//! most [`MAX_SETTING_LINE_CHARS`] on one line, [`MAX_SETTING_TEXT_CHARS`] where `multiline`, no
//! control character but the line breaks of a multi-line one), spelled `key`, `type`, `values`
//! as `docs/plugins.md` ("The manifest") writes them.

use anyhow::{Result, bail};

use super::{SettingKind, SettingManifest};

/// Most settings one plugin declares, and most values one choice offers.
const MAX_SETTINGS: usize = 16;
const MAX_SETTING_CHOICES: usize = 32;
/// Longest key and longest choice value.
const MAX_SETTING_KEY_CHARS: usize = 32;

/// Most characters a one-line text setting holds.
pub const MAX_SETTING_LINE_CHARS: usize = 256;

/// Most characters a multi-line text setting holds.
pub const MAX_SETTING_TEXT_CHARS: usize = 4096;

impl SettingManifest {
    /// Whether `value` is one this setting accepts: one of its values, or text within the rule.
    #[must_use]
    pub fn accepts(&self, value: &str) -> bool {
        match self.kind {
            SettingKind::Choice => self.values.iter().any(|choice| choice == value),
            SettingKind::Text => {
                let limit = if self.multiline {
                    MAX_SETTING_TEXT_CHARS
                } else {
                    MAX_SETTING_LINE_CHARS
                };
                value.chars().count() <= limit
                    && value.chars().all(|character| {
                        !character.is_control()
                            || (self.multiline && matches!(character, '\n' | '\r'))
                    })
            }
        }
    }
}

/// What `settings.setting` answers for a plugin declaring `declared`, given the person's
/// `stored` values `(key, value)` (RC-100-14): per declared setting the stored value while the
/// setting still accepts it, else its default; one with neither is left out (`none`), and so is
/// every stored key the manifest does not declare. As text, as rDownloader's
/// `destination-settings` hands them over.
pub(crate) fn resolve_settings(
    declared: &[SettingManifest],
    stored: &[(String, String)],
) -> Vec<(String, String)> {
    declared
        .iter()
        .filter_map(|setting| {
            let chosen = stored
                .iter()
                .find(|(key, value)| *key == setting.key && setting.accepts(value))
                .map(|(_, value)| value.clone());
            chosen
                .or_else(|| setting.default.clone())
                .map(|value| (setting.key.clone(), value))
        })
        .collect()
}

/// Keys that can be a setting's name and a code, values that can be offered.
pub(super) fn validate_settings(settings: &[SettingManifest]) -> Result<()> {
    if settings.len() > MAX_SETTINGS {
        bail!("[[settings]] declares more than {MAX_SETTINGS} settings");
    }
    let mut keys = Vec::new();
    for setting in settings {
        let key = setting.key.as_str();
        if key.is_empty()
            || key.len() > MAX_SETTING_KEY_CHARS
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
        {
            bail!("settings key `{key}` must be 1-{MAX_SETTING_KEY_CHARS} of a-z, 0-9 and _");
        }
        if keys.contains(&key) {
            bail!("settings key `{key}` is declared twice");
        }
        keys.push(key);
        match setting.kind {
            SettingKind::Choice => validate_choice(setting)?,
            SettingKind::Text => {
                if !setting.values.is_empty() {
                    bail!("settings `{key}` is text, so it lists no values");
                }
            }
        }
        if let Some(default) = &setting.default
            && (default.is_empty() || !setting.accepts(default))
        {
            bail!("settings `{key}` defaults to `{default}`, which it would refuse");
        }
    }
    Ok(())
}

fn validate_choice(setting: &SettingManifest) -> Result<()> {
    let key = &setting.key;
    if setting.multiline || setting.masked {
        bail!("settings `{key}` is a choice, so it cannot be multiline or masked");
    }
    if setting.values.is_empty() || setting.values.len() > MAX_SETTING_CHOICES {
        bail!("settings `{key}` needs 1-{MAX_SETTING_CHOICES} values");
    }
    for (index, value) in setting.values.iter().enumerate() {
        if value.is_empty()
            || value.trim() != value
            || value.chars().count() > MAX_SETTING_KEY_CHARS
            || value.chars().any(char::is_control)
            || setting.values[..index].contains(value)
        {
            bail!("settings `{key}` offers `{value}` badly or twice");
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
