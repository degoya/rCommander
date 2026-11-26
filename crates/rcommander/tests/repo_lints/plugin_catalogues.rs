//! Guards the bundled plugins' translations (RC-110-85): every plugin under `plugins/` (a folder
//! with a `manifest.toml`) ships `locales/en.json` and the three catalogues the app has beside
//! it - `de.json`, `fr.json`, `es.json` - and no other; each translation has English's `name`,
//! `description` and `codes`, each once, with the same `{placeholders}` and no empty text.
//!
//! The host asks a plugin for `en.json` only (`rc-plugin-host/src/locales.rs`), since a
//! third-party plugin may speak only English; the plugins rCommander ships speak every language
//! it does, or a German window shows an English viewer's name, settings and errors.

use std::{collections::BTreeMap, path::Path};

use serde::Deserialize;

use crate::{
    catalogues::{Entries, TRANSLATIONS, faults},
    workspace_root,
};

/// The catalogue every plugin ships and the others are compared with.
const ENGLISH: &str = "en";

/// One plugin catalogue, as the host reads it, its codes in file order.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Catalogue {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    codes: Option<Entries>,
}

/// A plugin catalogue as the entries `catalogues::faults` compares: `name`, `description` and
/// every code under its own key.
pub(crate) fn entries(text: &str) -> Result<Vec<(String, String)>, String> {
    let catalogue: Catalogue = serde_json::from_str(text).map_err(|error| error.to_string())?;
    let mut entries = Vec::new();
    for (key, value) in [
        ("name", catalogue.name),
        ("description", catalogue.description),
    ] {
        if let Some(value) = value {
            entries.push((key.to_owned(), value));
        }
    }
    entries.extend(catalogue.codes.map(|codes| codes.0).unwrap_or_default());
    Ok(entries)
}

/// What is wrong with the catalogues of `plugin`, given as language to file text.
fn plugin_faults(plugin: &str, files: &BTreeMap<String, String>) -> Vec<String> {
    let mut found = Vec::new();
    let shipped: Vec<&str> = files.keys().map(String::as_str).collect();
    let mut wanted: Vec<&str> = std::iter::once(ENGLISH)
        .chain(TRANSLATIONS.iter().copied())
        .collect();
    wanted.sort_unstable();
    if shipped != wanted {
        found.push(format!(
            "{plugin}: locales/ holds {shipped:?}, a bundled plugin ships {wanted:?}"
        ));
    }
    let Some(english) = files.get(ENGLISH) else {
        return found;
    };
    let english = match entries(english) {
        Ok(english) => english,
        Err(error) => {
            found.push(format!("{plugin}/locales/{ENGLISH}.json: {error}"));
            return found;
        }
    };
    for lang in TRANSLATIONS {
        let Some(text) = files.get(*lang) else {
            continue;
        };
        let name = format!("{plugin}/locales/{lang}.json");
        match entries(text) {
            Ok(other) => found.extend(faults(&name, &english, &other)),
            Err(error) => found.push(format!("{name}: {error}")),
        }
    }
    found
}

/// The `locales/<lang>.json` files of the plugin in `directory`, language to text.
fn catalogues(directory: &Path) -> BTreeMap<String, String> {
    let Ok(entries) = std::fs::read_dir(directory.join("locales")) else {
        return BTreeMap::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let lang = name.strip_suffix(".json")?.to_owned();
            let text = std::fs::read_to_string(entry.path()).unwrap_or_else(|error| {
                panic!("{} is not readable: {error}", entry.path().display())
            });
            Some((lang, text))
        })
        .collect()
}

#[test]
fn every_bundled_plugin_ships_every_catalogue() {
    let plugins = workspace_root().join("plugins");
    let mut directories: Vec<_> = std::fs::read_dir(&plugins)
        .expect("plugins/ is readable")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.join("manifest.toml").is_file())
        .collect();
    directories.sort();
    assert!(!directories.is_empty(), "no plugin found under plugins/");
    let found: Vec<String> = directories
        .iter()
        .flat_map(|directory| {
            let plugin = directory
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            plugin_faults(&plugin, &catalogues(directory))
        })
        .collect();
    assert!(
        found.is_empty(),
        "bundled plugins' catalogues are not whole:\n{}",
        found.join("\n")
    );
}

/// The lint fails on what it is for: a catalogue missing, a code missing, a placeholder lost and
/// a file the host would refuse.
#[test]
fn the_plugin_lint_reports_every_kind_of_fault() {
    fn catalogue(lang: &str, codes: &str) -> (String, String) {
        let text = format!(r#"{{"name": "X viewer ({lang})", "codes": {{{codes}}}}}"#);
        (lang.to_owned(), text)
    }
    let whole = r#""x.setting.encoding": "Encoding", "x.too_large": "Over {limit}.""#;
    let good: BTreeMap<String, String> = ["en", "de", "fr", "es"]
        .into_iter()
        .map(|lang| catalogue(lang, whole))
        .collect();
    assert!(plugin_faults("x-viewer", &good).is_empty());
    let bad: BTreeMap<String, String> = [
        catalogue("en", whole),
        catalogue("de", r#""x.too_large": "Too large.""#),
        (
            "fr".to_owned(),
            r#"{"name": "X viewer (fr)", "title": "X"}"#.to_owned(),
        ),
    ]
    .into_iter()
    .collect();
    let found = plugin_faults("x-viewer", &bad);
    assert_eq!(found.len(), 4, "{found:#?}");
    assert_eq!(
        found[..3],
        [
            r#"x-viewer: locales/ holds ["de", "en", "fr"], a bundled plugin ships ["de", "en", "es", "fr"]"#,
            "x-viewer/locales/de.json: 1 keys missing (en.json has them): x.setting.encoding",
            "x-viewer/locales/de.json: 1 placeholders differ from en.json: x.too_large",
        ]
    );
    assert!(
        found[3].starts_with("x-viewer/locales/fr.json: unknown field `title`"),
        "{}",
        found[3]
    );
}
