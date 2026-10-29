use std::fs;
use std::path::{Path, PathBuf};

use super::{expand_glob, expand_tilde, expand_tokens, has_wildcard, wildcard_match};

#[test]
fn a_leading_tilde_becomes_the_home_directory() {
    let home = Path::new("/home/tester");
    assert_eq!(expand_tilde("~", home), "/home/tester");
    assert_eq!(expand_tilde("~/.ssh/id", home), "/home/tester/.ssh/id");
    assert_eq!(expand_tilde(r"~\.ssh\id", home), r"/home/tester\.ssh\id");
    assert_eq!(
        expand_tilde("~/x", Path::new("/home/tester/")),
        "/home/tester/x"
    );
    assert_eq!(expand_tilde("~/x", Path::new("/")), "/x");
}

#[test]
fn a_tilde_elsewhere_or_with_a_user_stays() {
    let home = Path::new("/home/tester");
    assert_eq!(expand_tilde("~other/.ssh/id", home), "~other/.ssh/id");
    assert_eq!(expand_tilde("/keys/~/id", home), "/keys/~/id");
    assert_eq!(expand_tilde("id_rsa", home), "id_rsa");
}

#[test]
fn known_tokens_expand_and_a_double_percent_is_one() {
    let tokens = [
        ('d', "/home/tester"),
        ('h', "db.example.com"),
        ('u', "tester"),
    ];
    let mut unknown = Vec::new();
    assert_eq!(
        expand_tokens("%d/.ssh/%u@%h", &tokens, &mut unknown),
        "/home/tester/.ssh/tester@db.example.com"
    );
    assert_eq!(expand_tokens("100%%", &tokens, &mut unknown), "100%");
    assert_eq!(expand_tokens("plain", &tokens, &mut unknown), "plain");
    assert!(unknown.is_empty());
}

#[test]
fn unknown_tokens_stay_and_are_reported() {
    let mut unknown = Vec::new();
    assert_eq!(
        expand_tokens("%d/%C/x%", &[('d', "/h")], &mut unknown),
        "/h/%C/x%"
    );
    assert_eq!(unknown, vec!["%C".to_owned(), "%".to_owned()]);
}

#[test]
fn wildcards_match_like_a_shell_glob() {
    assert!(has_wildcard("config.d/*"));
    assert!(has_wildcard("host?"));
    assert!(!has_wildcard("config.d/work"));

    assert!(wildcard_match("*", ""));
    assert!(wildcard_match("*", "anything"));
    assert!(wildcard_match("*.conf", "work.conf"));
    assert!(!wildcard_match("*.conf", "work.conf.bak"));
    assert!(wildcard_match("a*b*c", "aXXbYYbZc"));
    assert!(!wildcard_match("a*b*c", "aXXbYY"));
    assert!(wildcard_match("host?", "host1"));
    assert!(!wildcard_match("host?", "host"));
    assert!(!wildcard_match("host?", "host12"));
    assert!(wildcard_match("**x", "x"));
    assert!(!wildcard_match("abc", "abd"));
}

fn touch(path: &Path) {
    fs::write(path, "").expect("write a fixture file");
}

#[test]
fn a_glob_lists_matching_files_sorted_without_hidden_ones() {
    let dir = tempfile::tempdir().expect("temp dir");
    let conf = dir.path().join("config.d");
    fs::create_dir(&conf).expect("create config.d");
    for name in ["b.conf", "a.conf", ".hidden.conf", "notes.txt"] {
        touch(&conf.join(name));
    }
    fs::create_dir(conf.join("dir.conf")).expect("create a directory that matches");

    let found = expand_glob(&conf.join("*.conf"));
    assert_eq!(found, vec![conf.join("a.conf"), conf.join("b.conf")]);

    let hidden = expand_glob(&conf.join(".*.conf"));
    assert_eq!(hidden, vec![conf.join(".hidden.conf")]);

    assert!(expand_glob(&conf.join("*.none")).is_empty());
}

#[test]
fn a_glob_may_sit_in_a_directory_component() {
    let dir = tempfile::tempdir().expect("temp dir");
    for sub in ["one", "two"] {
        fs::create_dir(dir.path().join(sub)).expect("create a subdirectory");
        touch(&dir.path().join(sub).join("config"));
    }
    let found = expand_glob(&dir.path().join("*").join("config"));
    assert_eq!(
        found,
        vec![
            dir.path().join("one").join("config"),
            dir.path().join("two").join("config"),
        ]
    );
}

#[test]
fn a_literal_path_comes_back_unchecked() {
    let path = PathBuf::from("/no/such/dir/config");
    assert_eq!(expand_glob(&path), vec![path]);
}
