//! Guards the i18n rule: every key a `t!("...")` names in `crates/rcommander/src` exists in
//! `crates/rcommander/locales/en.json`, and every key of the catalogue is named by one. The
//! exception are the `error.<code>` keys: the window looks them up by a stable error code at
//! run time (`i18n::error_text`), and `i18n_tests::every_error_code_has_a_message` holds them
//! against the code lists of the crates. Likewise the `setting.` keys: the settings dialog looks
//! them up through the engine's schema, and `app/settings_tests.rs` holds them against it. No key
//! is in the catalogue twice: a JSON map keeps the last of two equal keys without a word, so one
//! of two texts would never show (RC-080-12). A key only a test (`*_tests.rs`) names counts as
//! unused: the window never shows it (RC-080-54).

use std::{collections::BTreeSet, fmt, path::Path};

use serde::de::{Deserialize, Deserializer, MapAccess, Visitor};

use crate::{files_with, relative, workspace_root};

/// The keys looked up by a code rather than by name: error codes, the codes the engine answers
/// with for the window to translate (a checksum line's status, an ssh_config import's status,
/// skip reason and warning, a Total Commander import's page, a WSL shell's label and
/// start hint), and the names of the keymap's
/// rows and the command palette's entries (`keys.<name>`, held against the keymap by
/// `app/palette_tests.rs`), and the menu bar's titles and entries (`menubar.<id>`, held against
/// its menus by `app/palette/menu_bar_tests.rs`).
const CODE_KEYS: &[&str] = &[
    "error.",
    "checksum.invalid.",
    "checksum.status.",
    "keys.",
    "menubar.",
    "ssh_import.",
    "tc_import.",
    "terminal.shell.",
    "terminal.wsl.",
];

/// The keys looked up through the settings schema rather than by name.
const SCHEMA_KEYS: &str = "setting.";

/// The keys `t!("...")` names in `text`, in order - also where rustfmt put the key on the next
/// line. Comment lines (the macro's own documentation) are not code.
fn keys_in(text: &str) -> Vec<String> {
    let code = text
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    code.match_indices("t!(")
        .filter(|(at, _)| {
            // `t!`, not the end of `format!` or `assert!`.
            let before = code[..*at].chars().next_back();
            !before.is_some_and(|c| c.is_alphanumeric() || c == '_')
        })
        .filter_map(|(at, call)| {
            let argument = code[at + call.len()..].trim_start().strip_prefix('"')?;
            argument.split('"').next().map(str::to_owned)
        })
        .collect()
}

/// Whether `file` holds tests (`*_tests.rs`, `tests.rs`): a key only a test names is still
/// unused by the window (RC-080-54).
fn is_test_file(file: &str) -> bool {
    let name = file.rsplit('/').next().unwrap_or(file);
    name == "tests.rs" || name.ends_with("_tests.rs")
}

/// The keys missing from `catalogue` (as `<file>: <key>`) and the catalogue's keys no source
/// but a test names, for `sources` given as (file, text).
fn check(sources: &[(String, String)], catalogue: &[String]) -> (Vec<String>, Vec<String>) {
    let mut used = Vec::new();
    let mut missing = Vec::new();
    for (file, text) in sources {
        for key in keys_in(text) {
            if !catalogue.contains(&key) {
                missing.push(format!("{file}: {key}"));
            }
            if !is_test_file(file) {
                used.push(key);
            }
        }
    }
    let unused = catalogue
        .iter()
        .filter(|key| {
            !CODE_KEYS.iter().any(|prefix| key.starts_with(prefix))
                && !key.starts_with(SCHEMA_KEYS)
                && !used.contains(key)
        })
        .cloned()
        .collect();
    (missing, unused)
}

/// The keys of a catalogue in file order, a key given twice listed twice - what a map, which keeps
/// only the last, cannot tell.
struct FileKeys(Vec<String>);

impl<'de> Deserialize<'de> for FileKeys {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Keys;
        impl<'de> Visitor<'de> for Keys {
            type Value = FileKeys;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("an object of keys to strings")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<FileKeys, A::Error> {
                let mut keys = Vec::new();
                while let Some((key, _text)) = map.next_entry::<String, String>()? {
                    keys.push(key);
                }
                Ok(FileKeys(keys))
            }
        }
        deserializer.deserialize_map(Keys)
    }
}

/// The keys of the catalogue `text`, in file order, duplicates kept.
fn keys_of(text: &str) -> Vec<String> {
    serde_json::from_str::<FileKeys>(text)
        .expect("locales/en.json maps keys to strings")
        .0
}

/// The keys given more than once in `keys`, each named once.
fn duplicates(keys: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut twice = Vec::new();
    for key in keys {
        if !seen.insert(key) && !twice.contains(key) {
            twice.push(key.clone());
        }
    }
    twice
}

fn catalogue(root: &Path) -> Vec<String> {
    let path = root.join("crates/rcommander/locales/en.json");
    let text = std::fs::read_to_string(&path).expect("locales/en.json is readable");
    keys_of(&text)
}

#[test]
fn every_key_in_the_sources_exists_and_every_key_is_used() {
    let root = workspace_root();
    let mut files = Vec::new();
    files_with(&root.join("crates/rcommander/src"), &[".rs"], &mut files);
    assert!(!files.is_empty(), "expected to scan the window's sources");
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|path| {
            let text = std::fs::read_to_string(path).expect("a source is UTF-8");
            (relative(&root, path), text)
        })
        .collect();
    let (missing, unused) = check(&sources, &catalogue(&root));
    assert!(missing.is_empty(), "keys missing in en.json: {missing:?}");
    assert!(unused.is_empty(), "keys in en.json no t! names: {unused:?}");
}

/// The lint fails on what it is for: a key the catalogue lacks, and a key nobody uses.
#[test]
fn the_lint_reports_a_missing_and_an_unused_key() {
    let source = r#"let title = t!("app.title"); let wrong = t!("app.no_such_key", n = 1);
        let not_ours = format!("x"); assert!(true);
        /// `t!("key")` in a comment is no lookup.
        let wrapped = t!(
            "app.wrapped",
            n = 2,
        );"#;
    let sources = vec![("src/demo.rs".to_owned(), source.to_owned())];
    let catalogue: Vec<String> = [
        "app.title",
        "app.wrapped",
        "app.forgotten",
        "error.not_found",
    ]
    .map(str::to_owned)
    .into();
    let (missing, unused) = check(&sources, &catalogue);
    assert_eq!(missing, ["src/demo.rs: app.no_such_key"]);
    assert_eq!(unused, ["app.forgotten"]);
}

/// A key only a test names counts as unused (RC-080-54): the old scan took the tests' keys as
/// the window's. A test that names a key the catalogue lacks still fails.
#[test]
fn a_key_only_a_test_names_is_unused() {
    let sources = vec![
        (
            "src/app/demo.rs".to_owned(),
            r#"t!("app.title")"#.to_owned(),
        ),
        (
            "src/app/demo_tests.rs".to_owned(),
            r#"t!("app.forgotten"); t!("app.gone")"#.to_owned(),
        ),
    ];
    let catalogue: Vec<String> = ["app.title", "app.forgotten"].map(str::to_owned).into();
    let (missing, unused) = check(&sources, &catalogue);
    assert_eq!(missing, ["src/app/demo_tests.rs: app.gone"]);
    assert_eq!(unused, ["app.forgotten"]);
    assert!(is_test_file("crates/x/src/tests.rs") && !is_test_file("crates/x/src/tested.rs"));
}

#[test]
fn no_key_is_in_the_catalogue_twice() {
    let twice = duplicates(&catalogue(&workspace_root()));
    assert!(
        twice.is_empty(),
        "keys in en.json more than once, only the last text shows: {twice:?}"
    );
}

/// The lint fails on a key given twice, which a map would silently fold into one.
#[test]
fn the_lint_reports_a_key_given_twice() {
    let keys = keys_of(r#"{"error.timed_out": "a", "app.title": "b", "error.timed_out": "c"}"#);
    assert_eq!(keys, ["error.timed_out", "app.title", "error.timed_out"]);
    assert_eq!(duplicates(&keys), ["error.timed_out"]);
    assert!(duplicates(&keys_of(r#"{"a": "x", "b": "y"}"#)).is_empty());
}
