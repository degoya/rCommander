//! Guards French punctuation (RC-110-87): in `crates/rcommander/locales/fr.json` and every bundled
//! plugin's `locales/fr.json`, a `:` of prose has U+00A0 NO-BREAK SPACE before it, a `?`, `!` or
//! `;` U+202F NARROW NO-BREAK SPACE, and guillemets hold U+00A0 inside - so no line ever starts
//! with the mark. Punctuation is prose where whitespace, the end of the text or a closing quote
//! follows it and a letter, a digit, a placeholder's `}` or a closing mark comes before it (a
//! space between allowed); after any other character it is literal syntax - `.*:`, `< > : " | ?
//! *`, `::`, `(!)` - and takes no space. [`COMMAND_LINES`] are texts that are a command line to
//! copy, whose `;` stays bare.

use std::path::Path;

use crate::{catalogues, plugin_catalogues, workspace_root};

/// The space before `:` and inside guillemets.
const NO_BREAK: char = '\u{a0}';
/// The space before `?`, `!` and `;`.
const NARROW: char = '\u{202f}';

/// The texts that are a command line to copy, whose `;` separates commands.
const COMMAND_LINES: &[&str] = &["cli.plugin_next_steps"];

/// Whether `c` ends a word of prose: a letter, a digit, a placeholder's `}`, a closing mark.
fn ends_prose(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '}' | ')' | '\u{bb}' | '"' | '\'')
}

/// The marks of `text` without their space, each shown with what stands before it.
fn faults(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let shown = |at: usize| -> String { chars[at.saturating_sub(12)..=at].iter().collect() };
    let mut found: Vec<String> = Vec::new();
    for (at, &c) in chars.iter().enumerate() {
        let before = at.checked_sub(1).map(|i| chars[i]);
        let after = chars.get(at + 1).copied();
        match c {
            ':' | '?' | '!' | ';' => {
                if !after.is_none_or(|next| next.is_whitespace() || matches!(next, '"' | '\u{bb}'))
                {
                    continue;
                }
                let (space, word) = match before {
                    Some(space) if space.is_whitespace() => {
                        (Some(space), at.checked_sub(2).map(|i| chars[i]))
                    }
                    other => (None, other),
                };
                let wanted = if c == ':' { NO_BREAK } else { NARROW };
                if word.is_some_and(ends_prose) && space != Some(wanted) {
                    found.push(shown(at));
                }
            }
            '\u{ab}' if after != Some(NO_BREAK) => found.push(shown(at)),
            '\u{bb}' if before != Some(NO_BREAK) => found.push(shown(at)),
            _ => {}
        }
    }
    found
}

/// The faults of `entries` from the catalogue `name`, as `<name> <key>: <marks>`.
fn catalogue_faults(name: &str, entries: &[(String, String)]) -> Vec<String> {
    entries
        .iter()
        .filter(|(key, _)| !COMMAND_LINES.contains(&key.as_str()))
        .filter_map(|(key, text)| {
            let marks = faults(text);
            (!marks.is_empty()).then(|| format!("{name} {key}: {marks:?}"))
        })
        .collect()
}

/// The French catalogues of the bundled plugins, as (plugin, entries).
fn french_plugin_catalogues(root: &Path) -> Vec<(String, Vec<(String, String)>)> {
    let Ok(directories) = std::fs::read_dir(root.join("plugins")) else {
        return Vec::new();
    };
    let mut found: Vec<(String, Vec<(String, String)>)> = directories
        .flatten()
        .map(|entry| entry.path())
        .filter_map(|path| {
            let text = std::fs::read_to_string(path.join("locales/fr.json")).ok()?;
            let entries = plugin_catalogues::entries(&text).ok()?;
            Some((path.file_name()?.to_string_lossy().into_owned(), entries))
        })
        .collect();
    found.sort();
    found
}

/// Every French text spaces its punctuation as French does.
#[test]
fn french_punctuation_has_its_space() {
    let root = workspace_root();
    let mut found = catalogue_faults("fr.json", &catalogues::entries(&root, "fr"));
    for (plugin, entries) in french_plugin_catalogues(&root) {
        found.extend(catalogue_faults(&format!("{plugin}/fr.json"), &entries));
    }
    assert!(
        found.is_empty(),
        "{} French texts lack the no-break space before : or the narrow one before ? ! ; \
         (or inside guillemets):\n{}",
        found.len(),
        found.join("\n")
    );
}

/// The lint fails on a missing or a plain space and passes literal syntax.
#[test]
fn the_typography_lint_tells_prose_from_syntax() {
    assert!(faults("Nom\u{a0}: {name}\u{202f}? Oui\u{202f}!").is_empty());
    assert!(
        faults("\u{ab}\u{a0}Copier\u{a0}\u{bb} ou (?i)mot.*: d:) ::1 < > : \" | ? *").is_empty()
    );
    assert_eq!(faults("Nom: x"), ["Nom:"]);
    assert_eq!(faults("Pourquoi ?"), ["Pourquoi ?"]);
    assert_eq!(faults("{count} ; {more}"), ["{count} ;"]);
    assert_eq!(faults("\u{ab}Copier\u{bb}").len(), 2);
    let entries = [("cli.plugin_next_steps".to_owned(), "a; b".to_owned())];
    assert!(catalogue_faults("fr.json", &entries).is_empty());
}
