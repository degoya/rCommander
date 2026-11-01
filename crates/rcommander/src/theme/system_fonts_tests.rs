use super::{MAX_NAME, MONO_FALLBACKS, SANS_FALLBACKS, choose, parse_family};
use crate::theme::fonts::Family;

fn family(name: &str, mono: bool) -> Family {
    Family {
        name: name.to_owned(),
        mono,
    }
}

/// A Debian without Open Sans: Nimbus Sans as fontconfig's choice, DejaVu beside it.
fn debian() -> Vec<Family> {
    vec![
        family("DejaVu Sans", false),
        family("DejaVu Sans Mono", true),
        family("Nimbus Mono PS", true),
        family("Nimbus Sans", false),
    ]
}

#[test]
fn a_valid_fc_match_output_is_the_family() {
    assert_eq!(parse_family(b"Nimbus Sans"), Some("Nimbus Sans"));
    assert_eq!(
        parse_family(b"DejaVu Sans Mono\n"),
        Some("DejaVu Sans Mono")
    );
    assert_eq!(parse_family(b"  Noto Sans  "), Some("Noto Sans"));
}

#[test]
fn an_empty_or_nonsense_output_is_no_family() {
    assert_eq!(parse_family(b""), None);
    assert_eq!(parse_family(b" \n\t"), None);
    assert_eq!(parse_family(b"Nimbus Sans\nDejaVu Sans"), None, "two lines");
    assert_eq!(
        parse_family(b"Bad\x1b[31mName"),
        None,
        "a control character"
    );
    assert_eq!(parse_family(b"\xff\xfe"), None, "not UTF-8");
    let long = "A".repeat(MAX_NAME + 1);
    assert_eq!(parse_family(long.as_bytes()), None, "too long");
    let longest = "A".repeat(MAX_NAME);
    assert_eq!(parse_family(longest.as_bytes()), Some(longest.as_str()));
}

#[test]
fn the_system_family_wins_when_it_is_installed() {
    let installed = debian();
    assert_eq!(
        choose(Some("Nimbus Sans"), &SANS_FALLBACKS, &installed, false),
        Some("Nimbus Sans")
    );
    assert_eq!(
        choose(Some("Nimbus Mono PS"), &MONO_FALLBACKS, &installed, true),
        Some("Nimbus Mono PS")
    );
}

#[test]
fn the_database_spelling_is_set() {
    let installed = debian();
    assert_eq!(
        choose(Some("nimbus sans"), &SANS_FALLBACKS, &installed, false),
        Some("Nimbus Sans")
    );
}

#[test]
fn an_unknown_system_family_falls_back_to_the_first_installed() {
    let installed = debian();
    assert_eq!(
        choose(Some("Open Sans"), &SANS_FALLBACKS, &installed, false),
        Some("DejaVu Sans")
    );
    assert_eq!(
        choose(None, &MONO_FALLBACKS, &installed, true),
        Some("DejaVu Sans Mono")
    );
    // The list's order decides, not the database's.
    let both = vec![family("Cantarell", false), family("Liberation Sans", false)];
    assert_eq!(
        choose(None, &SANS_FALLBACKS, &both, false),
        Some("Liberation Sans")
    );
}

#[test]
fn only_known_families_are_set() {
    assert_eq!(
        choose(Some("Nimbus Sans"), &SANS_FALLBACKS, &[], false),
        None
    );
    let other = vec![family("Comic Neue", false)];
    assert_eq!(
        choose(Some("Open Sans"), &SANS_FALLBACKS, &other, false),
        None
    );
    assert_eq!(choose(None, &MONO_FALLBACKS, &other, true), None);
}

#[test]
fn a_monospace_family_needs_a_monospaced_face() {
    let installed = vec![
        family("Nimbus Sans", false),
        family("Liberation Mono", true),
    ];
    assert_eq!(
        choose(Some("Nimbus Sans"), &MONO_FALLBACKS, &installed, true),
        Some("Liberation Mono")
    );
}
