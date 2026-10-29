use rc_engine::Chord;

use super::*;

const LABELS: [Option<&str>; 5] = [
    Some("Copy"),
    Some("Move"),
    None,
    Some("*cache"),
    Some("Delete"),
];

#[test]
fn a_letter_only_one_entry_starts_with_takes_it() {
    assert_eq!(letter("m", LABELS, 0), Letter::Take(1));
    assert_eq!(letter("D", LABELS, 0), Letter::Take(4), "case is ignored");
}

#[test]
fn a_letter_several_entries_start_with_moves_to_the_next_wrapping() {
    // "Copy" and "*cache": the mark before a locked tab's title does not count.
    assert_eq!(letter("c", LABELS, 0), Letter::Move(3));
    assert_eq!(letter("c", LABELS, 3), Letter::Move(0), "wraps around");
    assert_eq!(letter("c", LABELS, 4), Letter::Move(0));
}

#[test]
fn a_letter_no_entry_starts_with_does_nothing_and_headings_never_match() {
    assert_eq!(letter("x", LABELS, 0), Letter::None);
    assert_eq!(letter("h", [None, Some("Hotlist")], 0), Letter::Take(1));
    assert_eq!(letter("", LABELS, 0), Letter::None);
}

#[test]
fn only_a_plain_or_shifted_character_is_a_letter() {
    let chord = |text: &str| -> Chord { text.parse().expect("a chord") };
    assert!(is_letter(&chord("c")));
    assert!(is_letter(&chord("Shift+c")));
    assert!(!is_letter(&chord("Ctrl+c")));
    assert!(!is_letter(&chord("Alt+c")));
    assert!(!is_letter(&chord("Enter")));
}
