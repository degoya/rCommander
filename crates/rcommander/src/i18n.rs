//! The interface's words, by key, from the catalogues in `locales/` compiled in.
//!
//! Every visible string goes through a key: `t!("key")`, or [`error_text`] for a stable error
//! code. A key is looked up in the language of the interface ([`crate::locale::current`]:
//! English, German, French or Spanish, RC-100-47) and, where that catalogue lacks it or holds an
//! empty text, in English; a key no catalogue has shows itself. The repo lint `i18n_keys` holds
//! the `t!` keys and `locales/en.json` against each other, `catalogues` every other catalogue
//! against `en.json`.

use std::{collections::HashMap, sync::OnceLock};

use crate::locale::{self, Lang};

/// One language's strings, by key.
type Catalogue = HashMap<String, String>;

/// The catalogue `text` in `cell`, parsed on first use. One that does not parse is empty and every
/// lookup then falls back to English (or shows its key); the tests keep that from shipping.
fn loaded(cell: &'static OnceLock<Catalogue>, text: &'static str) -> &'static Catalogue {
    cell.get_or_init(|| serde_json::from_str(text).unwrap_or_default())
}

/// The English catalogue, every key's fallback.
fn english() -> &'static Catalogue {
    static CATALOGUE: OnceLock<Catalogue> = OnceLock::new();
    loaded(&CATALOGUE, include_str!("../locales/en.json"))
}

/// The catalogue of `lang`.
fn catalogue(lang: Lang) -> &'static Catalogue {
    static GERMAN: OnceLock<Catalogue> = OnceLock::new();
    static FRENCH: OnceLock<Catalogue> = OnceLock::new();
    static SPANISH: OnceLock<Catalogue> = OnceLock::new();
    match lang {
        Lang::En => english(),
        Lang::De => loaded(&GERMAN, include_str!("../locales/de.json")),
        Lang::Fr => loaded(&FRENCH, include_str!("../locales/fr.json")),
        Lang::Es => loaded(&SPANISH, include_str!("../locales/es.json")),
    }
}

/// `key` in `primary`, or in `fallback` where `primary` lacks it or holds an empty text.
fn resolve<'a>(key: &str, primary: &'a Catalogue, fallback: &'a Catalogue) -> Option<&'a str> {
    primary
        .get(key)
        .filter(|text| !text.is_empty())
        .or_else(|| fallback.get(key))
        .map(String::as_str)
}

/// `key` in the interface's language, falling back to English.
fn find(key: &str) -> Option<&'static str> {
    resolve(key, catalogue(locale::current()), english())
}

/// The string `key`, or the key itself where no catalogue has it.
pub fn lookup(key: &'static str) -> &'static str {
    find(key).unwrap_or(key)
}

/// The string `key` with its `{placeholders}` filled from `values`.
pub fn fill(key: &'static str, values: &[(&str, String)]) -> String {
    values
        .iter()
        .fold(lookup(key).to_owned(), |text, (name, value)| {
            text.replace(&format!("{{{name}}}"), value)
        })
}

/// The message of a stable error code (the key `error.<code>`), or the code itself where no
/// catalogue has one.
pub fn error_text(code: &str) -> String {
    find(&format!("error.{code}")).map_or_else(|| code.to_owned(), str::to_owned)
}

/// The text of a code the engine answers for the window to translate (the key
/// `<prefix>.<code>`, such as `ssh_import.status.new`), or the code itself where no catalogue
/// has one.
pub fn code_text(prefix: &str, code: &str) -> String {
    find(&format!("{prefix}.{code}")).map_or_else(|| code.to_owned(), str::to_owned)
}

/// `t!("key")` is the string `key`; `t!("key", name = value, ...)` fills `{name}` placeholders.
macro_rules! t {
    ($key:literal) => {
        $crate::i18n::lookup($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::fill($key, &[$((stringify!($name), $value.to_string())),+])
    };
}
pub(crate) use t;

#[cfg(test)]
#[path = "i18n_tests.rs"]
mod tests;
