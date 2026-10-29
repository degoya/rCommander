use super::{NameMatcher, wildcard};

fn masks(pattern: &str) -> NameMatcher {
    NameMatcher::new(pattern, false, false).expect("masks always compile")
}

fn chars(text: &str) -> Vec<char> {
    text.chars().collect()
}

#[test]
fn wildcards_match_the_whole_name() {
    assert!(wildcard(&chars("*.rs"), chars("main.rs")));
    assert!(!wildcard(&chars("*.rs"), chars("main.rsx")));
    assert!(wildcard(&chars("a?c"), chars("abc")));
    assert!(!wildcard(&chars("a?c"), chars("ac")));
    assert!(wildcard(&chars("*a*b*"), chars("xxaxxbxx")));
    assert!(wildcard(&chars("*"), chars("")));
    assert!(!wildcard(&chars("?"), chars("")));
}

#[test]
fn several_masks_split_on_semicolons_and_spaces_with_quotes() {
    let matcher = masks(" *.rs  *.toml ;\"my file*.txt\"");
    assert!(matcher.matches("main.rs", false) && matcher.matches("Cargo.toml", false));
    assert!(matcher.matches("My File 2.txt", false));
    assert!(!matcher.matches("file.txt", false));
}

#[test]
fn an_include_mask_with_a_backslash_matches_directories_only() {
    let matcher = masks("src\\");
    assert!(matcher.matches("src", true));
    assert!(!matcher.matches("src", false));
}

#[test]
fn any_mask_of_the_list_matches_ignoring_case() {
    let matcher = masks("*.rs;*.TOML");
    assert!(matcher.matches("Main.RS", false));
    assert!(matcher.matches("Cargo.toml", false));
    assert!(!matcher.matches("README.md", false));
}

#[test]
fn umlauts_match_without_case_in_masks_and_exclusions() {
    let matcher = masks("\u{fc}b* *.\u{c4} | \u{d6}l* Stra\u{df}e\\");
    assert!(matcher.matches("\u{dc}BUNG.txt", false));
    assert!(matcher.matches("x.\u{e4}", false));
    assert!(!matcher.matches("\u{f6}lbild.txt", false));
    assert!(!matcher.enters("STRA\u{df}E"));
    assert!(matcher.enters("strasse"));
    let exact = NameMatcher::new("\u{dc}b*", false, true).expect("masks");
    assert!(exact.matches("\u{dc}bung", false));
    assert!(!exact.matches("\u{fc}bung", false));
}

#[test]
fn case_counts_when_asked() {
    let matcher = NameMatcher::new("*.RS", false, true).expect("masks");
    assert!(matcher.matches("A.RS", false));
    assert!(!matcher.matches("a.rs", false));
}

#[test]
fn a_mask_without_a_wildcard_matches_part_of_the_name() {
    let matcher = masks("report");
    assert!(matcher.matches("2026-Report-final.pdf", false));
    assert!(!matcher.matches("summary.pdf", false));
}

#[test]
fn star_dot_star_matches_names_without_a_dot() {
    let matcher = masks("*.*");
    assert!(matcher.matches("Makefile", false));
    assert!(matcher.matches("a.b", false));
}

#[test]
fn an_empty_list_matches_everything() {
    assert!(masks("").matches("anything", false));
    assert!(masks("| *.bak").matches("anything", false));
    assert!(!masks("| *.bak").matches("old.bak", false));
}

#[test]
fn exclusions_after_the_bar_remove_names() {
    let matcher = masks("*.txt | secret*");
    assert!(matcher.matches("notes.txt", false));
    assert!(!matcher.matches("secret.txt", false));
    assert!(
        matcher.enters("secret"),
        "a plain exclusion does not stop the walk"
    );
}

#[test]
fn a_directory_exclusion_is_neither_entered_nor_a_hit() {
    let matcher = masks("* | node_modules\\ .git/");
    assert!(!matcher.enters("node_modules"));
    assert!(!matcher.enters(".git"));
    assert!(matcher.enters("src"));
    assert!(!matcher.matches("node_modules", true));
    assert!(
        matcher.matches("node_modules", false),
        "a file of that name is not excluded"
    );
}

#[test]
fn a_regex_matches_anywhere_unless_anchored() {
    let matcher = NameMatcher::new(r"^\d{4}-.*\.log$", true, false).expect("regex");
    assert!(matcher.matches("2026-app.LOG", false));
    assert!(!matcher.matches("app-2026.log", false));
    assert!(matcher.enters("anything"));
}

#[test]
fn a_broken_regex_is_an_invalid_search() {
    let error = NameMatcher::new("(unclosed", true, false).expect_err("invalid");
    assert_eq!(error.code(), "invalid_search");
}

#[test]
fn an_exclusion_without_a_wildcard_is_taken_exactly() {
    let matcher = masks("| build target\\");
    assert!(!matcher.matches("build", false));
    assert!(matcher.matches("rebuild.sh", false));
    assert!(!matcher.enters("target"));
    assert!(matcher.enters("targets"));
}
