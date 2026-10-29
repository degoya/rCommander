//! Guards the English-only rule for Rust sources: user-facing text lives in the locale catalogue
//! (`crates/rcommander/locales/`) behind a key, never in code. Any German umlaut or sharp s in a
//! Rust source is a regression, and so is one of a few German words that need no umlaut.

use std::path::Path;

use crate::{files_with, relative, workspace_root};

const ALLOWED_FILES: &[&str] = &["crates/rcommander/tests/repo_lints/no_german.rs"];
const GERMAN_CHARS: &str = "\u{e4}\u{f6}\u{fc}\u{c4}\u{d6}\u{dc}\u{df}";
/// Whole words, case-sensitive: the nouns and verbs a German user text in a file manager is made
/// of and an English source never spells this way. Short words (`die`, `das`, `und`) are not
/// here; they are English words too.
const GERMAN_WORDS: &[&str] = &[
    "Abbrechen",
    "Bitte",
    "Datei",
    "Dateien",
    "Einstellungen",
    "Fehler",
    "Kopieren",
    "Laufwerk",
    "Ordner",
    "Passwort",
    "Speichern",
    "Umbenennen",
    "Verbindung",
    "Verschieben",
    "Verzeichnis",
];

fn is_german(line: &str) -> bool {
    line.chars().any(|c| GERMAN_CHARS.contains(c))
        || line
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| GERMAN_WORDS.contains(&word))
}

#[test]
fn rust_sources_contain_no_german_text() {
    let root = workspace_root();
    let mut sources = Vec::new();
    files_with(&root.join("crates"), &[".rs"], &mut sources);
    assert!(
        !sources.is_empty(),
        "expected to scan the workspace sources"
    );
    let offenders: Vec<String> = sources
        .iter()
        .filter(|path| {
            let relative = path.strip_prefix(&root).expect("relative");
            // `Path` equality compares components, so the `/` spelling matches a Windows `\`.
            !ALLOWED_FILES
                .iter()
                .any(|allowed| relative == Path::new(allowed))
        })
        .filter_map(|path| {
            let text = std::fs::read_to_string(path).ok()?;
            let (number, line) = text.lines().enumerate().find(|(_, line)| is_german(line))?;
            Some(format!(
                "{}:{}: {}",
                relative(&root, path),
                number + 1,
                line.trim()
            ))
        })
        .collect();
    assert!(
        offenders.is_empty(),
        "German text found:\n{}",
        offenders.join("\n")
    );
}

#[test]
fn a_german_word_without_an_umlaut_is_found_as_a_whole_word() {
    assert!(is_german(r#"        "Datei".to_owned()"#));
    assert!(is_german("// Ordner fehlt"));
    assert!(is_german("// \u{fc}ber"));
    assert!(!is_german(r#"        "folder".to_owned()"#));
    assert!(!is_german("let datei_count = 1; // Dateiname"));
}
