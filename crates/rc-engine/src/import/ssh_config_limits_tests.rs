//! RC-080-47: the reading of a configuration is bounded - each file once, at most
//! `MAX_INCLUDES` includes, `MAX_HOSTS` hosts, and per host its key paths and ignored keywords.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use super::{
    MAX_HOSTS, MAX_IDENTITY_FILES, MAX_IGNORED_OPTIONS, MAX_INCLUDES, SshConfigContext,
    SshConfigImport, WarningKind, load, parse_str,
};

fn context(ssh_dir: &Path) -> SshConfigContext {
    SshConfigContext {
        home: PathBuf::from("/home/tester"),
        ssh_dir: ssh_dir.to_path_buf(),
        local_user: "tester".to_owned(),
    }
}

fn parse(text: &str) -> SshConfigImport {
    let dir = Path::new("/nowhere");
    parse_str(text, Path::new("/virtual/config"), &context(dir)).expect("no key material")
}

fn count(found: &SshConfigImport, code: &str) -> usize {
    found
        .warnings
        .iter()
        .filter(|warning| warning.kind.code() == code)
        .count()
}

#[test]
fn a_file_included_again_is_read_once() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(dir.path().join("common"), "Host shared\nUser one\n").expect("write common");
    // Every line of `nest` names `common` four times and `nest` again: without the rule each
    // level would read them all once more.
    fs::write(
        dir.path().join("nest"),
        "Include common common common common nest\n",
    )
    .expect("write nest");
    let root = dir.path().join("config");
    fs::write(&root, "Include nest common\nHost after\nInclude common\n").expect("write root");

    let found = load(&root, &context(dir.path())).expect("reads");

    let aliases: Vec<&str> = found.hosts.iter().map(|host| host.alias.as_str()).collect();
    assert_eq!(aliases, ["shared", "after"]);
    assert_eq!(
        found.hosts[1].user, None,
        "common is not read under `after`"
    );
    assert_eq!(count(&found, "include_repeated"), 5);
    assert_eq!(count(&found, "include_cycle"), 1);
    let repeated = found
        .warnings
        .iter()
        .find_map(|warning| match &warning.kind {
            WarningKind::IncludeRepeated { file } => Some(file.clone()),
            _ => None,
        })
        .expect("a repeated include");
    assert_eq!(repeated, dir.path().join("common"));
}

#[test]
fn includes_past_the_limit_are_skipped_with_one_warning() {
    let dir = tempfile::tempdir().expect("temp dir");
    let many = dir.path().join("many");
    fs::create_dir(&many).expect("many");
    for n in 0..MAX_INCLUDES + 20 {
        fs::write(many.join(format!("f{n:04}")), format!("Host h{n}\n")).expect("write");
    }
    let root = dir.path().join("config");
    fs::write(&root, "Include many/*\nInclude many/f0000\nHost last\n").expect("write root");

    let found = load(&root, &context(dir.path())).expect("reads");

    assert_eq!(
        found.hosts.len(),
        MAX_INCLUDES + 1,
        "the includes and `last`"
    );
    assert_eq!(found.hosts[MAX_INCLUDES].alias, "last");
    assert_eq!(count(&found, "too_many_includes"), 1);
    assert_eq!(found.warnings.len(), 1);
}

#[test]
fn hosts_past_the_limit_are_not_taken_and_known_ones_still_are() {
    let mut text = String::from("Host first\n");
    for n in 0..MAX_HOSTS + 10 {
        writeln!(text, "Host h{n} pattern{n}*").expect("write");
    }
    text.push_str("Host FIRST\nUser late\n");

    let found = parse(&text);

    assert_eq!(found.hosts.len(), MAX_HOSTS);
    assert_eq!(found.skipped.len(), MAX_HOSTS);
    assert_eq!(found.hosts[0].user.as_deref(), Some("late"));
    assert_eq!(count(&found, "too_many_hosts"), 1);
}

#[test]
fn a_host_keeps_its_bounded_key_paths_and_ignored_keywords() {
    // One name short of the limit, so that `extra` is still taken.
    let names: Vec<String> = (0..MAX_HOSTS - 1).map(|n| format!("n{n}")).collect();
    let mut text = format!("Host {}\nUser first\n", names.join(" "));
    for n in 0..MAX_IDENTITY_FILES + 50 {
        writeln!(text, "IdentityFile /keys/{n}\nUser again{n}").expect("write");
    }
    for n in 0..MAX_IGNORED_OPTIONS + 50 {
        writeln!(text, "Option{n} yes").expect("write");
    }
    text.push_str("Host n0 extra\nHostName n0.example\nOption0 no\nOptionExtra yes\n");

    let found = parse(&text);

    assert_eq!(found.hosts.len(), MAX_HOSTS);
    for host in &found.hosts[..MAX_HOSTS - 1] {
        assert_eq!(host.user.as_deref(), Some("first"));
        assert_eq!(host.identity_files.len(), MAX_IDENTITY_FILES);
        assert_eq!(host.ignored_options.len(), MAX_IGNORED_OPTIONS);
    }
    let first = &found.hosts[0];
    assert_eq!(first.identity_files[0], PathBuf::from("/keys/0"));
    assert_eq!(first.address(), "n0.example");
    assert_eq!(first.ignored_options[0], "Option0");
    let extra = &found.hosts[MAX_HOSTS - 1];
    assert_eq!(extra.ignored_options, ["Option0", "OptionExtra"]);
    assert!(found.warnings.is_empty(), "{:?}", found.warnings);
}
