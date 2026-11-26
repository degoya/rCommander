//! Guards the texts with numbers (RC-110-91): a number a word of its text agrees with - "1 file",
//! "2 files", "1 fichier", "2 fichiers" - is the `{count}` of a pair, `<key>.one`/`<key>.other`
//! (RC-110-82), which a text holds once. A text with several such numbers is a frame whose
//! placeholders are parts, each a pair `<frame>.<part>` of its own that the window fills first
//! (`i18n::counted`). The lint knows such a number by the name of its placeholder, [`COUNTS`]: in
//! `en.json`, whose placeholders every translation shares, one of them is a fault unless the pair
//! `<frame>.<name>` stands beside its text, the frame being the key without its `.one`/`.other`.
//! A number no word agrees with - `{done} of {total}`, `{shown} of {count} crates`, a fixed limit -
//! has a name of its own outside the list.

use std::collections::BTreeSet;

use crate::{
    catalogues::{entries, listed, placeholders},
    workspace_root,
};

/// The placeholder names of numbers a word of their text agrees with.
const COUNTS: &[&str] = &[
    "added",
    "channels",
    "changes",
    "conflicts",
    "connections",
    "created",
    "damaged",
    "different",
    "dirs",
    "encrypted",
    "entries",
    "equal",
    "failed",
    "files",
    "files_done",
    "folders",
    "invalid",
    "left_only",
    "mismatch",
    "missing",
    "ok",
    "parts",
    "passed",
    "right_only",
    "seconds",
    "selected_dirs",
    "selected_files",
    "skipped",
    "unchanged",
    "unloadable",
    "unreadable",
    "updated",
];

/// The texts of `entries` that name a number of [`COUNTS`] which is no part, as `<key>: {name}`.
fn faults(entries: &[(String, String)]) -> Vec<String> {
    let keys: BTreeSet<&str> = entries.iter().map(|(key, _)| key.as_str()).collect();
    let mut found = Vec::new();
    for (key, text) in entries {
        let key = key.as_str();
        let frame = key
            .strip_suffix(".one")
            .or_else(|| key.strip_suffix(".other"))
            .unwrap_or(key);
        for name in placeholders(text) {
            let part = |form: &str| keys.contains(format!("{frame}.{name}.{form}").as_str());
            if COUNTS.contains(&name) && !(part("one") && part("other")) {
                found.push(format!("{key}: {{{name}}}"));
            }
        }
    }
    found
}

/// No text of `en.json` takes a counted number any way but through a pair.
#[test]
fn every_counted_number_is_a_pair() {
    let found = faults(&entries(&workspace_root(), "en"));
    let shown: Vec<&str> = found.iter().map(String::as_str).collect();
    assert!(
        found.is_empty(),
        "{} texts name a number a word agrees with outside a pair (RC-110-91: a part \
         `<frame>.<name>.one`/`.other`, filled by `i18n::counted`): {}",
        found.len(),
        listed(&shown)
    );
}

/// The lint fails on a raw number and on a part without its pair, and passes a frame, a pair and a
/// number no word agrees with.
#[test]
fn the_lint_reports_a_number_outside_a_pair() {
    let entries: Vec<(String, String)> = [
        ("props.sum", "{files}, {dirs}"),
        ("props.sum.files.one", "{count} file"),
        ("props.sum.files.other", "{count} files"),
        ("props.sum.dirs.one", "{count} folder"),
        ("props.sum.dirs.other", "{count} folders"),
        ("ops.loaded.one", "{count} job loaded; {failed}"),
        ("ops.loaded.other", "{count} jobs loaded; {failed}"),
        ("ops.loaded.failed.one", "{count} failed"),
        ("ops.loaded.failed.other", "{count} failed"),
        ("ops.live", "{done} of {total}"),
        ("search.found", "{ok} OK, {missing} missing"),
        ("search.half", "{files}"),
        ("search.half.files.one", "{count} file"),
    ]
    .iter()
    .map(|(key, text)| ((*key).to_owned(), (*text).to_owned()))
    .collect();
    assert_eq!(
        faults(&entries),
        [
            "search.found: {missing}",
            "search.found: {ok}",
            "search.half: {files}",
        ]
    );
}
