//! Guards the translations (RC-100-47): every catalogue in `crates/rcommander/locales/` beside
//! `en.json` - `de.json`, `fr.json`, `es.json`, each compiled in by `i18n.rs` - has exactly
//! `en.json`'s keys, each once, with the same `{placeholders}` per key and no empty text; and
//! the names of the languages, which the language setting shows, stay in their own language in
//! every catalogue. A key a translation lacks would fall back to English at run time; the lint
//! keeps that from shipping. A text that names a number is a pair, `<key>.one` and `<key>.other`
//! (RC-110-82): in every catalogue, `en.json` too, each form has its partner, both name
//! `{count}` - the value that chooses between them - and the `one` form names no placeholder the
//! `other` form lacks, since one call fills both.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    path::Path,
};

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};

use crate::workspace_root;

/// The catalogues `i18n.rs` compiles in beside English.
pub(crate) const TRANSLATIONS: &[&str] = &["de", "fr", "es"];

/// The keys whose text is the same in every catalogue: each language's name in that language.
const SAME_EVERYWHERE: &[&str] = &[
    "setting.appearance.language.en",
    "setting.appearance.language.de",
    "setting.appearance.language.fr",
    "setting.appearance.language.es",
];

/// The placeholder that chooses a pair's form, and the forms.
const COUNT: &str = "count";
const ONE: &str = ".one";
const OTHER: &str = ".other";

/// How many keys of one kind a failure lists before it only counts the rest.
const SHOWN: usize = 20;

/// A catalogue's entries in file order, a key given twice listed twice.
pub(crate) struct Entries(pub(crate) Vec<(String, String)>);

impl<'de> Deserialize<'de> for Entries {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Pairs;
        impl<'de> Visitor<'de> for Pairs {
            type Value = Entries;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an object of keys to strings")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Entries, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry::<String, String>()? {
                    entries.push(entry);
                }
                Ok(Entries(entries))
            }
        }
        deserializer.deserialize_map(Pairs)
    }
}

/// The `{name}` placeholders of `text`, each once.
pub(crate) fn placeholders(text: &str) -> BTreeSet<&str> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('}') else {
            break;
        };
        let name = &rest[..close];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            found.insert(name);
            rest = &rest[close + 1..];
        }
    }
    found
}

/// `keys` as one line: the first [`SHOWN`], then how many more.
pub(crate) fn listed(keys: &[&str]) -> String {
    let line = keys
        .iter()
        .take(SHOWN)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if keys.len() > SHOWN {
        format!("{line} ... and {} more", keys.len() - SHOWN)
    } else {
        line
    }
}

/// What is wrong with the catalogue `name` against `english`, one line per kind of fault.
pub(crate) fn faults(
    name: &str,
    english: &[(String, String)],
    other: &[(String, String)],
) -> Vec<String> {
    let mut faults = Vec::new();
    let mut seen = BTreeSet::new();
    let mut twice = Vec::new();
    for (key, _) in other {
        if !seen.insert(key.as_str()) {
            twice.push(key.as_str());
        }
    }
    // The last of two equal keys, as the window's map keeps it.
    let texts: BTreeMap<&str, &str> = other
        .iter()
        .map(|(key, text)| (key.as_str(), text.as_str()))
        .collect();
    let missing: Vec<&str> = english
        .iter()
        .map(|(key, _)| key.as_str())
        .filter(|key| !seen.contains(key))
        .collect();
    let known: BTreeSet<&str> = english.iter().map(|(key, _)| key.as_str()).collect();
    let extra: Vec<&str> = seen
        .iter()
        .copied()
        .filter(|key| !known.contains(key))
        .collect();
    let empty: Vec<&str> = other
        .iter()
        .filter(|(_, text)| text.trim().is_empty())
        .map(|(key, _)| key.as_str())
        .collect();
    let mut placeholders_differ = Vec::new();
    let mut not_same = Vec::new();
    for (key, text) in english {
        let Some(translated) = texts.get(key.as_str()) else {
            continue;
        };
        if placeholders(text) != placeholders(translated) {
            placeholders_differ.push(key.as_str());
        }
        if SAME_EVERYWHERE.contains(&key.as_str()) && text != translated {
            not_same.push(key.as_str());
        }
    }
    for (what, keys) in [
        ("keys missing (en.json has them)", missing),
        ("keys en.json does not have", extra),
        ("keys given more than once", twice),
        ("empty texts", empty),
        ("placeholders differ from en.json", placeholders_differ),
        ("language names not as in en.json", not_same),
    ] {
        if !keys.is_empty() {
            faults.push(format!("{name}: {} {what}: {}", keys.len(), listed(&keys)));
        }
    }
    faults
}

/// What is wrong with the pairs of the catalogue `name`: a form whose text names `{count}` and
/// whose partner is missing or names no `{count}`, and a `one` form with a placeholder its
/// `other` form lacks. A key ending in `.one` or `.other` without `{count}` is no form.
fn pair_faults(name: &str, entries: &[(String, String)]) -> Vec<String> {
    let texts: BTreeMap<&str, &str> = entries
        .iter()
        .map(|(key, text)| (key.as_str(), text.as_str()))
        .collect();
    let counted = |key: &str| {
        texts
            .get(key)
            .is_some_and(|text| placeholders(text).contains(COUNT))
    };
    let mut alone = Vec::new();
    let mut extra = Vec::new();
    for (&key, &text) in &texts {
        let partner = if let Some(stem) = key.strip_suffix(ONE) {
            format!("{stem}{OTHER}")
        } else if let Some(stem) = key.strip_suffix(OTHER) {
            format!("{stem}{ONE}")
        } else {
            continue;
        };
        if !counted(key) {
            continue;
        }
        if !counted(partner.as_str()) {
            alone.push(key);
        }
        // A partner without `{count}` is already reported as alone.
        if let (Some(stem), Some(other)) = (key.strip_suffix(ONE), texts.get(partner.as_str()))
            && counted(partner.as_str())
            && !placeholders(text).is_subset(&placeholders(other))
        {
            extra.push(stem);
        }
    }
    let mut faults = Vec::new();
    for (what, keys) in [
        ("plural forms without their partner (with {count})", alone),
        ("one forms with a placeholder the other form lacks", extra),
    ] {
        if !keys.is_empty() {
            faults.push(format!("{name}: {} {what}: {}", keys.len(), listed(&keys)));
        }
    }
    faults
}

/// The entries of the catalogue `<lang>.json`.
pub(crate) fn entries(root: &Path, lang: &str) -> Vec<(String, String)> {
    let path = root.join(format!("crates/rcommander/locales/{lang}.json"));
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("locales/{lang}.json is not readable: {error}"));
    serde_json::from_str::<Entries>(&text)
        .unwrap_or_else(|error| panic!("locales/{lang}.json does not map keys to strings: {error}"))
        .0
}

/// Strict: on a branch that carries only the placeholder `{}` of a catalogue, this lists every
/// key as missing - it passes once the translations are merged.
#[test]
fn every_translation_has_the_english_keys_and_placeholders() {
    let root = workspace_root();
    let english = entries(&root, "en");
    let found: Vec<String> = TRANSLATIONS
        .iter()
        .flat_map(|lang| faults(&format!("{lang}.json"), &english, &entries(&root, lang)))
        .collect();
    assert!(
        found.is_empty(),
        "catalogues differ from en.json:\n{}",
        found.join("\n")
    );
}

/// Every pair in every catalogue is whole (RC-110-82): a lone form would read its partner from
/// English, or show its key.
#[test]
fn every_plural_pair_is_whole() {
    let root = workspace_root();
    let found: Vec<String> = std::iter::once("en")
        .chain(TRANSLATIONS.iter().copied())
        .flat_map(|lang| pair_faults(&format!("{lang}.json"), &entries(&root, lang)))
        .collect();
    assert!(
        found.is_empty(),
        "plural pairs are not whole:\n{}",
        found.join("\n")
    );
}

/// Every catalogue file beside `en.json` is one `i18n.rs` compiles in: a new language is a
/// variant of `locale::Lang` and a line in both.
#[test]
fn every_catalogue_file_is_a_known_language() {
    let directory = workspace_root().join("crates/rcommander/locales");
    let mut found: Vec<String> = std::fs::read_dir(&directory)
        .expect("locales/ is readable")
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.strip_suffix(".json").map(str::to_owned)
        })
        .filter(|lang| lang != "en")
        .collect();
    found.sort();
    let mut known: Vec<&str> = TRANSLATIONS.to_vec();
    known.sort_unstable();
    assert_eq!(found, known);
}

/// The lint fails on what it is for: a missing key, an extra one, one given twice, an empty
/// text, a lost placeholder and a translated language name.
#[test]
fn the_lint_reports_every_kind_of_fault() {
    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(key, text)| ((*key).to_owned(), (*text).to_owned()))
            .collect()
    }
    let english = pairs(&[
        ("app.title", "rCommander"),
        ("copy.count", "Copy {count} files to {target}?"),
        ("app.quit", "Quit"),
        ("setting.appearance.language.de", "Deutsch"),
    ]);
    let good = pairs(&[
        ("app.title", "rCommander"),
        ("copy.count", "Copier {count} fichiers vers {target} ?"),
        ("app.quit", "Quitter"),
        ("setting.appearance.language.de", "Deutsch"),
    ]);
    assert!(faults("good.json", &english, &good).is_empty());
    let bad = pairs(&[
        ("app.title", " "),
        ("copy.count", "Copier {count} fichiers ?"),
        ("app.extra", "x"),
        ("app.extra", "y"),
        ("setting.appearance.language.de", "German"),
    ]);
    let found = faults("bad.json", &english, &bad);
    assert_eq!(
        found,
        [
            "bad.json: 1 keys missing (en.json has them): app.quit",
            "bad.json: 1 keys en.json does not have: app.extra",
            "bad.json: 1 keys given more than once: app.extra",
            "bad.json: 1 empty texts: app.title",
            "bad.json: 1 placeholders differ from en.json: copy.count",
            "bad.json: 1 language names not as in en.json: setting.appearance.language.de",
        ]
    );
    assert_eq!(
        placeholders("{a} {b_2} {} {not one} {a}"),
        BTreeSet::from(["a", "b_2"])
    );
}

/// The pair lint fails on a lone form, on a partner without `{count}` and on a `one` form that
/// names more than its `other` form; a key that only ends in `.other` is no form.
#[test]
fn the_lint_reports_a_broken_pair() {
    let entries: Vec<(String, String)> = [
        ("queue.marked.one", "{count} job selected"),
        ("queue.marked.other", "{count} jobs selected"),
        ("queue.title.other", "{name}: {count} jobs"),
        ("ops.failed.one", "{count} file failed"),
        ("ops.failed.other", "Files failed"),
        ("drop.refused.one", "{count} of {total} path was not taken"),
        ("drop.refused.other", "{count} paths were not taken"),
        ("queue.kind.other", "Other"),
    ]
    .iter()
    .map(|(key, text)| ((*key).to_owned(), (*text).to_owned()))
    .collect();
    assert_eq!(
        pair_faults("bad.json", &entries),
        [
            "bad.json: 2 plural forms without their partner (with {count}): ops.failed.one, \
             queue.title.other",
            "bad.json: 1 one forms with a placeholder the other form lacks: drop.refused",
        ]
    );
}
