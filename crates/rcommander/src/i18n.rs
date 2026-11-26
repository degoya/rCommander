//! The interface's words, by key, from the catalogues in `locales/` compiled in.
//!
//! Every visible string goes through a key: `t!("key")`, or [`error_text`] for a stable error
//! code. A key is looked up in the language of the interface ([`crate::locale::current`]:
//! English, German, French or Spanish, RC-100-47) and, where that catalogue lacks it or holds an
//! empty text, in English; a key no catalogue has shows itself. A text that names a number is a
//! pair, `<key>.one` and `<key>.other` (RC-110-82): `t!("key", count = n)` takes the form the
//! language's plural rule ([`plural_form`]) asks for `n`. The repo lint `i18n_keys` holds the
//! `t!` keys and `locales/en.json` against each other, `catalogues` every other catalogue
//! against `en.json` and every pair against itself. A text that names several numbers is a frame
//! of parts, each a pair of its own ([`counted`], RC-110-91).

use std::{collections::HashMap, sync::OnceLock};

use crate::locale::{self, Lang};

pub mod counted;

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

/// The placeholder whose value chooses a pair's plural form.
const COUNT: &str = "count";

/// The plural form a number takes in `lang`: CLDR's cardinal rule, of which the catalogues use
/// `one` and `other`. English, German and Spanish say `one` for 1 alone, French for 0 and 1; a
/// count that is no number takes `other`.
fn plural_form(lang: Lang, count: Option<u64>) -> &'static str {
    let one = match lang {
        Lang::Fr => matches!(count, Some(0 | 1)),
        Lang::En | Lang::De | Lang::Es => count == Some(1),
    };
    if one { "one" } else { "other" }
}

/// The number a `count` value reads as: its digits, whatever separates their groups
/// ([`crate::format::count`] puts a comma, a point or a narrow space between them, by language);
/// `None` without a digit.
fn number(value: &str) -> Option<u64> {
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// The pair `key.one`/`key.other` for `count`: the form `lang`'s rule asks for in `primary`,
/// else the form English's rule asks for in `fallback` - a French "one" is no English one at 0.
/// `None` where neither catalogue has the form, an empty text being none.
fn plural<'a>(
    key: &str,
    count: Option<u64>,
    lang: Lang,
    primary: &'a Catalogue,
    fallback: &'a Catalogue,
) -> Option<&'a str> {
    let form = |lang: Lang, texts: &'a Catalogue| {
        texts
            .get(&format!("{key}.{}", plural_form(lang, count)))
            .filter(|text| !text.is_empty())
            .map(String::as_str)
    };
    form(lang, primary).or_else(|| form(Lang::En, fallback))
}

/// The string `key` with its `{placeholders}` filled from `values`; with a `count` among them
/// and a pair under `key`, the pair's form for that count.
pub fn fill(key: &'static str, values: &[(&str, String)]) -> String {
    fill_in(locale::current(), key, values)
}

/// [`fill`] in `lang`: `t!(in lang; "key", ...)`, for the texts a test reads in every language.
pub(crate) fn fill_in(lang: Lang, key: &'static str, values: &[(&str, String)]) -> String {
    let (primary, fallback) = (catalogue(lang), english());
    let text = values
        .iter()
        .find(|(name, _)| *name == COUNT)
        .and_then(|(_, count)| plural(key, number(count), lang, primary, fallback))
        .or_else(|| resolve(key, primary, fallback))
        .unwrap_or(key);
    values.iter().fold(text.to_owned(), |text, (name, value)| {
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

/// `t!("key")` is the string `key`; `t!("key", name = value, ...)` fills `{name}` placeholders,
/// and `count = n` among them chooses the plural form of a pair `key.one`/`key.other`.
/// `t!(in lang; "key", name = value, ...)` is the same in `lang` rather than the interface's
/// language - [`counted`]'s texts, which their tests read in every language.
macro_rules! t {
    (in $lang:expr; $key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::fill_in($lang, $key, &[$((stringify!($name), $value.to_string())),+])
    };
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
