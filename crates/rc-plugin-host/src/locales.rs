//! Plugin-supplied translations shipped inside the signed package.
//!
//! Copied and adapted from rDownloader's `crates/rd-plugin-host/src/locales.rs` at commit
//! 26c8592a6, with its rules unchanged (`docs/plugins.md`, "Strings and i18n"): `locales/<lang>.json`
//! of at most [`MAX_LOCALE_BYTES`], at most [`MAX_LOCALE_FILES`], `en` required if any ships,
//! unknown keys refused, every code under the plugin's own namespace so a plugin can never
//! override rCommander's keys or another plugin's. rDownloader's account labels are not taken:
//! an rCommander plugin has no account form. Plugin text is data, never passed through `t!`.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

/// Per-file size cap; generous for a few hundred short strings.
pub const MAX_LOCALE_BYTES: u64 = 256 * 1024;
/// Upper bound on locale files in one package.
pub const MAX_LOCALE_FILES: usize = 16;
/// Every localised plugin ships English, the fallback.
pub const REQUIRED_LOCALE: &str = "en";

/// One `locales/<lang>.json` document.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PluginLocale {
    /// Localised display name, overriding the manifest's `name`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Localised description, overriding `[metadata] description`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Translations keyed exactly as the plugin emits them: failure codes, setting labels
    /// (`<slug>.setting.<key>`), choice labels.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub codes: BTreeMap<String, String>,
}

/// The language tag of a `locales/<lang>.json` member name; `None` for any other path, so
/// callers can refuse unexpected members.
#[must_use]
pub fn locale_member_language(name: &str) -> Option<&str> {
    let language = name.strip_prefix("locales/")?.strip_suffix(".json")?;
    valid_language(language).then_some(language)
}

/// Whether `language` is a bare two-letter tag: a member name can never leave `locales/` or
/// collide case-insensitively.
#[must_use]
pub fn valid_language(language: &str) -> bool {
    language.len() == 2 && language.bytes().all(|byte| byte.is_ascii_lowercase())
}

/// Parses and validates one locale document of the plugin whose codes start with `<slug>.`.
pub fn parse_locale(slug: &str, language: &str, bytes: &[u8]) -> Result<PluginLocale> {
    let locale: PluginLocale =
        serde_json::from_slice(bytes).with_context(|| format!("parse locales/{language}.json"))?;
    let prefix = format!("{slug}.");
    for (code, text) in &locale.codes {
        if !code.starts_with(&prefix) {
            bail!("locales/{language}.json code `{code}` must start with `{prefix}`");
        }
        if text.trim().is_empty() {
            bail!("locales/{language}.json code `{code}` has an empty translation");
        }
    }
    for (field, value) in [("name", &locale.name), ("description", &locale.description)] {
        if value.as_ref().is_some_and(|text| text.trim().is_empty()) {
            bail!("locales/{language}.json {field} is empty");
        }
    }
    Ok(locale)
}

/// Validates the whole locale set of one package.
pub fn validate_locales(slug: &str, locales: &[(String, Vec<u8>)]) -> Result<()> {
    if locales.is_empty() {
        return Ok(());
    }
    if locales.len() > MAX_LOCALE_FILES {
        bail!("plugin ships more than {MAX_LOCALE_FILES} locale files");
    }
    let mut seen = std::collections::BTreeSet::new();
    for (language, bytes) in locales {
        if !valid_language(language) {
            bail!("invalid locale language tag `{language}`");
        }
        if bytes.len() as u64 > MAX_LOCALE_BYTES {
            bail!("locales/{language}.json exceeds its size limit");
        }
        if !seen.insert(language.as_str()) {
            bail!("duplicate locale `{language}`");
        }
        parse_locale(slug, language, bytes)?;
    }
    if !seen.contains(REQUIRED_LOCALE) {
        bail!("a localised plugin must ship locales/{REQUIRED_LOCALE}.json");
    }
    Ok(())
}

#[cfg(test)]
#[path = "locales_tests.rs"]
mod tests;
