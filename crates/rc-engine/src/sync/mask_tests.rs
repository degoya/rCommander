use super::Masks;

#[test]
fn the_empty_mask_takes_everything() {
    let masks = Masks::parse("");
    assert!(masks.includes_file("anything.bin"));
    assert!(masks.includes_file("noext"));
    assert!(masks.includes_dir(".git"));
}

#[test]
fn star_dot_star_takes_files_without_an_extension_too() {
    let masks = Masks::parse("*.*");
    assert!(masks.includes_file("Makefile"));
    assert!(masks.includes_file("a.txt"));
}

#[test]
fn include_masks_ignore_case_and_take_wildcards() {
    let masks = Masks::parse("*.RS;Cargo.???");
    assert!(masks.includes_file("main.rs"));
    assert!(masks.includes_file("cargo.ini"));
    assert!(masks.includes_file("Cargo.LOC"));
    assert!(!masks.includes_file("Cargo.lock"));
    assert!(!masks.includes_file("readme.md"));
    // Include masks are about files; directories are still entered.
    assert!(masks.includes_dir("src"));
}

#[test]
fn exclude_masks_follow_the_bar_and_directory_masks_end_in_a_slash() {
    let masks = Masks::parse("* | *.bak *.tmp target\\ .git/");
    assert!(masks.includes_file("a.rs"));
    assert!(!masks.includes_file("a.BAK"));
    assert!(!masks.includes_file("x.tmp"));
    assert!(!masks.includes_dir("target"));
    assert!(!masks.includes_dir(".GIT"));
    // A directory mask leaves a file of that name alone, and a file mask a directory.
    assert!(masks.includes_file("target"));
    assert!(masks.includes_dir("old.bak"));
}

#[test]
fn only_an_exclude_part_takes_every_other_file() {
    let masks = Masks::parse("|*.log");
    assert!(masks.includes_file("a.txt"));
    assert!(!masks.includes_file("server.log"));
}

#[test]
fn quoted_masks_keep_their_spaces() {
    let masks = Masks::parse("\"my file*.txt\"");
    assert!(masks.includes_file("My File 2.txt"));
    assert!(!masks.includes_file("my.txt"));
}

#[test]
fn stars_backtrack() {
    let masks = Masks::parse("a*b*c");
    assert!(masks.includes_file("abbbxbc"));
    assert!(masks.includes_file("abc"));
    assert!(!masks.includes_file("abcd"));
    assert!(!masks.includes_file("acb"));
}
