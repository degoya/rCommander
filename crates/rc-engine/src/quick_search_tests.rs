use rc_vfs::{Attributes, Entry, EntryKind};

use super::*;

fn entry(name: &str) -> Entry {
    Entry {
        name: name.to_owned(),
        kind: EntryKind::File,
        size: 0,
        modified: None,
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

/// `Docs/`, `notes.txt`, `README.md`, `report.txt`, `todo.md`, `NOTES-old.txt`.
fn listing() -> Vec<Entry> {
    [
        "Docs",
        "notes.txt",
        "README.md",
        "report.txt",
        "todo.md",
        "NOTES-old.txt",
    ]
    .into_iter()
    .map(entry)
    .collect()
}

fn start(text: &str) -> NameMatch {
    NameMatch::quick_search(text, MatchOptions::default()).expect("a text")
}

fn anywhere(text: &str) -> NameMatch {
    let options = MatchOptions {
        anywhere: true,
        case_sensitive: false,
    };
    NameMatch::quick_search(text, options).expect("a text")
}

#[test]
fn a_quick_search_matches_the_start_of_a_name_ignoring_case() {
    let re = start("re");
    assert!(re.matches("README.md"));
    assert!(re.matches("report.txt"));
    assert!(!re.matches("score.txt"), "not inside the name");
    assert!(NameMatch::quick_search("", MatchOptions::default()).is_none());
}

#[test]
fn anywhere_matches_inside_the_name() {
    let ote = anywhere("ote");
    assert!(ote.matches("notes.txt"));
    assert!(ote.matches("NOTES-old.txt"));
    assert!(!ote.matches("todo.md"));
}

#[test]
fn umlauts_match_without_case_unless_case_counts() {
    assert!(start("\u{c4}R").matches("\u{e4}rger.txt"));
    assert!(anywhere("\u{f6}").matches("SCH\u{d6}N.md"));
    assert!(
        NameMatch::filter("\u{dc}B")
            .expect("a filter")
            .matches("\u{fc}bung.txt")
    );
    let options = MatchOptions {
        anywhere: false,
        case_sensitive: true,
    };
    let upper = NameMatch::quick_search("\u{c4}r", options).expect("a text");
    assert!(upper.matches("\u{c4}rger.txt"));
    assert!(!upper.matches("\u{e4}rger.txt"));
}

#[test]
fn case_counts_when_the_setting_says_so() {
    let options = MatchOptions {
        anywhere: false,
        case_sensitive: true,
    };
    let upper = NameMatch::quick_search("NO", options).expect("a text");
    assert!(upper.matches("NOTES-old.txt"));
    assert!(!upper.matches("notes.txt"));
}

#[test]
fn wildcards_work_in_a_quick_search() {
    let star = start("*.md");
    assert!(star.matches("todo.md"));
    assert!(star.matches("README.md"));
    assert!(!star.matches("notes.txt"));
    let one = start("n?tes");
    assert!(one.matches("notes.txt"));
    assert!(!one.matches("nutmeg"));
}

#[test]
fn the_cursor_stays_while_its_entry_matches_else_goes_to_the_first_match() {
    let entries = listing();
    // On `report.txt` (3), "re" still matches: it stays.
    assert_eq!(find(&entries, &start("re"), 3, Seek::Here), Some(3));
    // "rea" does not: the first match from the top, `README.md`.
    assert_eq!(find(&entries, &start("rea"), 3, Seek::Here), Some(2));
    assert_eq!(find(&entries, &start("zzz"), 3, Seek::Here), None);
}

#[test]
fn arrows_step_between_matches_and_stop_at_the_last() {
    let entries = listing();
    let notes = start("no");
    // `notes.txt` (1) and `NOTES-old.txt` (5).
    assert_eq!(find(&entries, &notes, 0, Seek::Here), Some(1));
    assert_eq!(find(&entries, &notes, 1, Seek::Next), Some(5));
    assert_eq!(find(&entries, &notes, 5, Seek::Next), None, "no wrap");
    assert_eq!(find(&entries, &notes, 5, Seek::Previous), Some(1));
    assert_eq!(find(&entries, &notes, 1, Seek::Previous), None);
}

#[test]
fn an_empty_listing_or_a_cursor_past_the_end_finds_nothing_or_starts_over() {
    assert_eq!(find(&[], &start("a"), 0, Seek::Here), None);
    let entries = listing();
    assert_eq!(find(&entries, &start("todo"), 99, Seek::Here), Some(4));
    assert_eq!(find(&entries, &start("todo"), 99, Seek::Previous), Some(4));
}

#[test]
fn a_filter_without_wildcards_keeps_names_containing_it() {
    let filter = NameMatch::filter("OTE").expect("a filter");
    let kept: Vec<String> = filtered(&listing(), &filter)
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(kept, ["notes.txt", "NOTES-old.txt"]);
}

#[test]
fn a_filter_with_wildcards_matches_the_whole_name() {
    let names = |pattern: &str| -> Vec<String> {
        let filter = NameMatch::filter(pattern).expect("a filter");
        filtered(&listing(), &filter)
            .into_iter()
            .map(|entry| entry.name)
            .collect()
    };
    assert_eq!(names("*.md"), ["README.md", "todo.md"]);
    assert_eq!(names("*.TXT"), ["notes.txt", "report.txt", "NOTES-old.txt"]);
    assert_eq!(names("t?do*"), ["todo.md"]);
    assert_eq!(names("d*"), ["Docs"]);
}

#[test]
fn a_blank_filter_is_none() {
    assert!(NameMatch::filter("").is_none());
    assert!(NameMatch::filter("   ").is_none());
}
