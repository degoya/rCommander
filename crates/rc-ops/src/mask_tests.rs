use super::{Mask, Token, lowercase, split_line, tokens, wildcard};

fn chars(text: &str) -> Vec<char> {
    text.chars().collect()
}

fn files(mask: &str, names: &[&str]) -> Vec<String> {
    let mask = Mask::parse(mask);
    names
        .iter()
        .filter(|name| mask.matches(name, false))
        .map(|name| (*name).to_owned())
        .collect()
}

const NAMES: &[&str] = &[
    "a.txt",
    "B.TXT",
    "notes.md",
    "readme",
    "read.me.txt",
    "x1.log",
];

#[test]
fn wildcard_matches_the_whole_name_as_given() {
    assert!(wildcard(&chars("*.rs"), chars("main.rs")));
    assert!(!wildcard(&chars("*.rs"), chars("main.rsx")));
    assert!(wildcard(&chars("a?c"), chars("abc")));
    assert!(!wildcard(&chars("a?c"), chars("ac")));
    assert!(wildcard(&chars("*a*b*"), chars("xxaxxbxx")));
    assert!(wildcard(&chars("*"), chars("")));
    assert!(!wildcard(&chars("?"), chars("")));
    assert!(!wildcard(&chars("A*"), chars("abc")), "case counts");
}

#[test]
fn wildcard_stays_linear_on_many_stars() {
    let pattern = chars(&"*a".repeat(64));
    let name = chars(&"a".repeat(10_000));
    assert!(wildcard(&pattern, &name));
    assert!(!wildcard(&chars(&format!("{}b", "*a".repeat(64))), &name));
}

#[test]
fn a_name_is_lowered_and_matched_without_an_allocation() {
    assert!(wildcard(&chars("*\u{e4}r.*"), lowercase("B\u{c4}R.TXT")));
    let pattern: Vec<char> = lowercase("*STRA?E*").collect();
    assert!(wildcard(&pattern, lowercase("Stra\u{df}e.pdf")));
    assert!(
        !wildcard(&chars("*\u{e4}r.*"), "B\u{c4}R.TXT".chars()),
        "as given"
    );
    let mask = Mask::parse("\u{dc}B*;*.\u{d6}");
    assert!(mask.matches("\u{fc}bung.txt", false) && mask.matches("x.\u{f6}", false));
    assert!(!mask.matches("bung.txt", false));
}

#[test]
fn wildcards_match_without_case() {
    assert_eq!(files("*.txt", NAMES), ["a.txt", "B.TXT", "read.me.txt"]);
    assert_eq!(files("?.txt", NAMES), ["a.txt", "B.TXT"]);
    assert_eq!(files("x?.*", NAMES), ["x1.log"]);
    assert_eq!(files("read*", NAMES), ["readme", "read.me.txt"]);
    assert_eq!(files("*e*e*", NAMES), ["readme", "read.me.txt"]);
    assert_eq!(files("readme", NAMES), ["readme"]);
}

#[test]
fn star_dot_star_matches_every_file_even_without_a_dot() {
    assert_eq!(files("*.*", NAMES), NAMES);
    assert_eq!(files("*", NAMES), NAMES);
}

#[test]
fn several_masks_and_exclusions() {
    assert_eq!(files("*.md; *.log", NAMES), ["notes.md", "x1.log"]);
    assert_eq!(files("*.txt|read*", NAMES), ["a.txt", "B.TXT"]);
    assert_eq!(files("|*.txt", NAMES), ["notes.md", "readme", "x1.log"]);
    assert!(files("", NAMES).is_empty());
}

#[test]
fn a_trailing_backslash_matches_directories_only() {
    let mask = Mask::parse("*\\");
    assert!(mask.matches("src", true));
    assert!(!mask.matches("a.txt", false));
    let files_only = Mask::parse("*.*");
    assert!(!files_only.matches("src", true));
    let both = Mask::parse("s*;s*/");
    assert!(both.matches("src", true) && both.matches("setup.py", false));
}

#[test]
fn parse_or_all_takes_every_file_for_an_empty_include_side() {
    for text in ["*.*", "*", "", "  ", "|"] {
        let mask = Mask::parse_or_all(text);
        assert!(mask.matches("README", false), "{text:?}");
        assert!(mask.matches("a.txt", false), "{text:?}");
    }
}

#[test]
fn parse_or_all_keeps_masks_and_exclusions() {
    let mask = Mask::parse_or_all("*.txt;*.MD|read*");
    assert!(mask.matches("notes.txt", false));
    assert!(mask.matches("guide.md", false));
    assert!(!mask.matches("readme.md", false));
    assert!(!mask.matches("image.png", false));
    let rest = Mask::parse_or_all("|*.tmp");
    assert!(rest.matches("a.txt", false));
    assert!(!rest.matches("b.TMP", false));
    let one = Mask::parse_or_all("file?.bin");
    assert!(one.matches("file1.bin", false));
    assert!(!one.matches("file10.bin", false));
}

#[test]
fn with_folders_a_line_without_a_directory_mask_matches_directories_too() {
    let all = Mask::parse("*.*").with_folders();
    assert!(all.matches("src", true) && all.matches("a.txt", false));
    let some = Mask::parse("s*|*.bak").with_folders();
    assert!(some.matches("src", true) && some.matches("setup.py", false));
    assert!(!some.matches("docs", true));
    assert!(
        !some.matches("save.bak", true),
        "exclusions count for directories too"
    );
    // A directory mask keeps the line as it is: its file masks stay files only.
    let explicit = Mask::parse("*\\").with_folders();
    assert!(explicit.matches("src", true) && !explicit.matches("a.txt", false));
    let mixed = Mask::parse("*.txt;src\\").with_folders();
    assert!(mixed.matches("src", true) && !mixed.matches("old.txt", true));
    let excluded_dir = Mask::parse("*|tmp\\").with_folders();
    assert!(!excluded_dir.matches("docs", true));
    assert!(!Mask::parse("").with_folders().matches("src", true));
}

fn texts(list: &str) -> Vec<String> {
    tokens(list).into_iter().map(|token| token.text).collect()
}

#[test]
fn the_tokenizer_splits_on_semicolons_and_white_space_with_quotes() {
    assert_eq!(texts("*.rs;*.toml"), ["*.rs", "*.toml"]);
    assert_eq!(texts(" *.rs  *.toml ;\t"), ["*.rs", "*.toml"]);
    assert_eq!(texts("\"my file.txt\" *.md"), ["my file.txt", "*.md"]);
    assert_eq!(texts("\"a;b\""), ["a;b"], "quotes keep a semicolon too");
    assert!(tokens("  ; ").is_empty());
}

#[test]
fn the_tokenizer_marks_directory_masks_and_reads_star_dot_star_as_star() {
    let dir = |text: &str| Token {
        text: text.to_owned(),
        dir: true,
    };
    assert_eq!(tokens("target\\ .git/"), [dir("target"), dir(".git")]);
    assert_eq!(tokens("\\"), [dir("*")]);
    assert_eq!(texts("*.* *.*\\"), ["*", "*"]);
    let (include, exclude) = split_line("*.txt|read* a|b");
    assert_eq!(include.len(), 1);
    assert_eq!(exclude.len(), 2, "only the first bar splits");
}

#[test]
fn a_mask_line_takes_spaces_and_quotes_as_total_commander_does() {
    assert_eq!(files("*.md *.log", NAMES), ["notes.md", "x1.log"]);
    let quoted = Mask::parse("\"my file*.txt\"");
    assert!(quoted.matches("My File 2.txt", false));
    assert!(!quoted.matches("my.txt", false));
}
