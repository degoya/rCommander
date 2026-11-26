use std::collections::BTreeSet;

use super::super::licenses::packages;
use super::{INDEX, OWN, SHOWN_BYTES, TEXTS, Text, for_filter, index, parse};

const REGENERATE: &str = "run scripts/licenses.sh";

/// The acceptance criterion: every crate the page lists has its licence text in the file.
#[test]
fn every_listed_crate_has_a_text() {
    let missing: Vec<String> = packages()
        .iter()
        .filter(|package| {
            !INDEX
                .by_crate
                .contains_key(&(package.name.as_str(), package.version.as_str()))
        })
        .map(|package| format!("{}@{}", package.name, package.version))
        .collect();
    assert!(
        missing.is_empty(),
        "without a licence text: {missing:?}; {REGENERATE}"
    );
}

#[test]
fn every_text_covers_listed_crates_only() {
    let listed: BTreeSet<(&str, &str)> = packages()
        .iter()
        .map(|package| (package.name.as_str(), package.version.as_str()))
        .collect();
    let stale: Vec<_> = INDEX
        .by_crate
        .keys()
        .filter(|key| !listed.contains(*key))
        .collect();
    assert!(
        stale.is_empty(),
        "texts of unlisted crates: {stale:?}; {REGENERATE}"
    );
    assert!(
        INDEX.texts.iter().all(|text| !text.body.trim().is_empty()),
        "an empty licence text; {REGENERATE}"
    );
}

#[test]
fn the_preamble_counts_the_crates_and_the_texts() {
    let counts = format!(
        "Crates: {}. Distinct licence texts: {}.",
        packages().len(),
        INDEX.texts.len()
    );
    assert!(TEXTS.contains(&counts), "{counts}; {REGENERATE}");
}

#[test]
fn a_file_parses_into_its_blocks() {
    let file = "Preamble\n=== Licence\n\n\
                === Licence ======\n\
                a 1.0.0 (MIT): LICENSE\n\
                b 2.0.0 (MIT OR Apache-2.0): LICENSE-MIT\n\
                --- Text ------\n\
                Permission is granted.\n\nSecond paragraph.\n\
                \n=== Licence ======\n\
                b 2.0.0 (MIT OR Apache-2.0): LICENSE-APACHE\n\
                --- Text ------\n\
                Apache License\n";
    let texts = parse(file);
    assert_eq!(
        texts,
        [
            Text {
                covers: vec![
                    "a 1.0.0 (MIT): LICENSE",
                    "b 2.0.0 (MIT OR Apache-2.0): LICENSE-MIT"
                ],
                body: "Permission is granted.\n\nSecond paragraph.",
            },
            Text {
                covers: vec!["b 2.0.0 (MIT OR Apache-2.0): LICENSE-APACHE"],
                body: "Apache License",
            },
        ]
    );
    let index = index(texts);
    assert_eq!(index.by_crate[&("a", "1.0.0")], [0]);
    assert_eq!(index.by_crate[&("b", "2.0.0")], [0, 1]);
}

#[test]
fn the_filter_shows_its_crates_texts_within_the_cap() {
    let (iced, _) = for_filter("iced");
    assert!(
        iced.iter()
            .any(|text| text.covers.iter().any(|line| line.starts_with("iced "))),
        "iced's text is not shown"
    );
    let (all, more) = for_filter("");
    let bytes: usize = all.iter().map(|text| text.body.len()).sum();
    assert!(!all.is_empty() && bytes <= SHOWN_BYTES.max(all[0].body.len()));
    assert_eq!(
        all.len() + more,
        INDEX.texts.len() + 1,
        "and rCommander's own"
    );
    assert_eq!(for_filter("no such crate or licence"), (Vec::new(), 0));
}

/// rCommander's own licence leads the texts while the filter is empty or names it (RC-110-75).
#[test]
fn rcommanders_own_licence_comes_first() {
    assert!(
        OWN.body
            .trim_start()
            .starts_with("GNU GENERAL PUBLIC LICENSE"),
        "the GPL's text"
    );
    assert_eq!(
        OWN.covers,
        [concat!(
            "rCommander ",
            env!("CARGO_PKG_VERSION"),
            " (GPL-3.0-or-later): LICENSE"
        )]
    );
    for filter in ["", "  ", "rCommander", "commander", "gpl-3.0"] {
        let (shown, _) = for_filter(filter);
        assert!(
            shown.first().is_some_and(|text| std::ptr::eq(*text, &*OWN)),
            "{filter:?}: the own licence first"
        );
    }
    let (mit, _) = for_filter("mit");
    assert!(
        mit.iter().all(|text| !std::ptr::eq(*text, &*OWN)),
        "a filter that names neither leaves it out"
    );
}
