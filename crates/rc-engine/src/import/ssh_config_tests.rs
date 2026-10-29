use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::{
    ImportWarning, ImportedHost, LoadError, MAX_INCLUDE_DEPTH, SkipReason, SkippedHost, SourceLine,
    SshConfigContext, SshConfigImport, WarningKind, load, parse_str,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ssh_config")
}

/// A user `tester` at `/home/tester` whose `.ssh` directory is `ssh_dir`.
fn context(ssh_dir: &Path) -> SshConfigContext {
    SshConfigContext {
        home: PathBuf::from("/home/tester"),
        ssh_dir: ssh_dir.to_path_buf(),
        local_user: "tester".to_owned(),
    }
}

fn at(file: &Path, line: usize) -> SourceLine {
    SourceLine {
        file: file.to_path_buf(),
        line,
    }
}

fn host(alias: &str, source: SourceLine) -> ImportedHost {
    ImportedHost {
        alias: alias.to_owned(),
        host_name: None,
        user: None,
        port: None,
        identity_files: Vec::new(),
        proxy_jump: None,
        ignored_options: Vec::new(),
        source,
    }
}

fn some(text: &str) -> Option<String> {
    Some(text.to_owned())
}

fn parse(text: &str) -> SshConfigImport {
    parse_str(text, Path::new("/virtual/config"), &context(&fixtures())).expect("no key material")
}

fn virtual_line(line: usize) -> SourceLine {
    at(Path::new("/virtual/config"), line)
}

#[test]
fn the_fixture_yields_exactly_the_expected_hosts_skips_and_warnings() {
    let dir = fixtures();
    let config = dir.join("config");
    let work = dir.join("config.d/10-work.conf");
    let home = dir.join("config.d/20-home.conf");

    let found = load(&config, &context(&dir)).expect("the fixture reads");

    let expected = vec![
        ImportedHost {
            host_name: some("a.work.example"),
            user: some("alice"),
            ..host("work-a", at(&work, 1))
        },
        ImportedHost {
            host_name: some("192.168.1.20"),
            port: Some(22),
            ..host("pi", at(&home, 1))
        },
        ImportedHost {
            host_name: some("web1.example.com"),
            user: some("deploy"),
            ..host("web1", at(&config, 8))
        },
        ImportedHost {
            host_name: some("web2.example.com"),
            user: some("deploy"),
            ..host("web2", at(&config, 8))
        },
        ImportedHost {
            host_name: some("10.0.0.5"),
            port: Some(2222),
            identity_files: vec![
                PathBuf::from("/home/tester/.ssh/id_db"),
                PathBuf::from("/home/tester/keys/tester@10.0.0.5"),
            ],
            ..host("db", at(&config, 12))
        },
        ImportedHost {
            host_name: some("bastion.example.com"),
            user: some("jump"),
            ..host("bastion", at(&config, 18))
        },
        ImportedHost {
            host_name: some("10.1.0.7"),
            proxy_jump: some("bastion"),
            ignored_options: vec!["ForwardAgent".to_owned()],
            ..host("internal", at(&config, 22))
        },
        ImportedHost {
            host_name: some("github.com"),
            user: some("git"),
            identity_files: vec![PathBuf::from("/home/tester/.ssh/id_ed25519")],
            ignored_options: vec!["IdentitiesOnly".to_owned()],
            ..host("git", at(&config, 30))
        },
        ImportedHost {
            host_name: some("mixed.example.com"),
            port: Some(2200),
            ..host("Mixed", at(&config, 36))
        },
        ImportedHost {
            host_name: some("nas.local"),
            ..host("nas", at(&config, 43))
        },
    ];
    assert_eq!(found.hosts, expected);
    assert_eq!(found.hosts.len(), 10);

    let skipped = vec![
        SkippedHost {
            name: "*".to_owned(),
            reason: SkipReason::Pattern,
            source: at(&config, 4),
        },
        SkippedHost {
            name: "*.corp".to_owned(),
            reason: SkipReason::Pattern,
            source: at(&config, 27),
        },
        SkippedHost {
            name: "!secret.corp".to_owned(),
            reason: SkipReason::Negated,
            source: at(&config, 27),
        },
    ];
    assert_eq!(found.skipped, skipped);

    let warnings = vec![ImportWarning {
        source: at(&config, 40),
        kind: WarningKind::MatchSkipped,
    }];
    assert_eq!(found.warnings, warnings);
}

#[test]
fn an_include_cycle_is_reported_and_not_followed() {
    let dir = fixtures();
    let a = dir.join("cycle/a");
    let b = dir.join("cycle/b");

    let found = load(&a, &context(&dir)).expect("the fixture reads");

    let aliases: Vec<&str> = found.hosts.iter().map(|h| h.alias.as_str()).collect();
    assert_eq!(aliases, ["cycle-a", "cycle-b"]);
    let warnings = vec![ImportWarning {
        source: at(&b, 2),
        kind: WarningKind::IncludeCycle { file: a },
    }];
    assert_eq!(found.warnings, warnings);
}

#[test]
fn an_include_deeper_than_the_limit_is_reported() {
    let dir = tempfile::tempdir().expect("temp dir");
    let file = |n: usize| dir.path().join(format!("f{n}"));
    for n in 0..=MAX_INCLUDE_DEPTH + 1 {
        let text = format!("Host h{n}\nInclude f{}\n", n + 1);
        fs::write(file(n), text).expect("write a chain file");
    }

    let found = load(&file(0), &context(dir.path())).expect("the chain reads");

    // f0 is the root, f1 to f8 nest one to eight deep, f9 would be nine.
    assert_eq!(found.hosts.len(), MAX_INCLUDE_DEPTH + 1);
    let deepest = MAX_INCLUDE_DEPTH + 1;
    let warnings = vec![ImportWarning {
        source: at(&file(deepest - 1), 2),
        kind: WarningKind::IncludeTooDeep {
            file: file(deepest),
        },
    }];
    assert_eq!(found.warnings, warnings);
}

#[test]
fn a_missing_literal_include_warns_and_an_empty_glob_does_not() {
    let found = parse("Include missing.conf\nInclude config.d/*.none\nInclude ~/nowhere/*\n");
    let warnings = vec![ImportWarning {
        source: virtual_line(1),
        kind: WarningKind::IncludeNotFound {
            file: fixtures().join("missing.conf"),
        },
    }];
    assert_eq!(found.warnings, warnings);
    assert!(found.hosts.is_empty());
}

#[test]
fn an_include_inside_a_host_block_continues_that_block() {
    let dir = tempfile::tempdir().expect("temp dir");
    fs::write(
        dir.path().join("common"),
        "User shared\nHost other\nPort 2022\n",
    )
    .expect("write the included file");
    let text = "Host first\nInclude common\nPort 2200\nHostName first.example\n";

    let found =
        parse_str(text, Path::new("/virtual/config"), &context(dir.path())).expect("no key");

    let first = &found.hosts[0];
    assert_eq!(first.user.as_deref(), Some("shared"));
    assert_eq!(first.port, Some(2200));
    assert_eq!(first.host_name.as_deref(), Some("first.example"));
    let other = &found.hosts[1];
    assert_eq!((other.alias.as_str(), other.port), ("other", Some(2022)));
    assert!(found.warnings.is_empty());
}

#[test]
fn a_match_block_and_its_include_are_skipped_up_to_the_next_host() {
    let found = parse("Match all\nUser nobody\nInclude config.d/*.conf\nHost after\nUser me\n");
    assert_eq!(found.hosts.len(), 1);
    assert_eq!(found.hosts[0].user.as_deref(), Some("me"));
    let kinds: Vec<&WarningKind> = found.warnings.iter().map(|w| &w.kind).collect();
    assert_eq!(
        kinds,
        [&WarningKind::MatchSkipped, &WarningKind::IncludeInMatch]
    );
}

#[test]
fn the_first_value_wins_and_aliases_merge_regardless_of_case() {
    let found = parse(concat!(
        "Host app\n",
        "  User first\n",
        "  User second\n",
        "  IdentityFile ~/.ssh/one\n",
        "Host APP other\n",
        "  User third\n",
        "  HostName app.example\n",
        "  IdentityFile ~/.ssh/two\n",
        "  Compression yes\n",
        "  compression no\n",
    ));
    let app = &found.hosts[0];
    assert_eq!(found.hosts.len(), 2);
    assert_eq!(app.alias, "app");
    assert_eq!(app.user.as_deref(), Some("first"));
    assert_eq!(app.address(), "app.example");
    assert_eq!(
        app.identity_files,
        [
            PathBuf::from("/home/tester/.ssh/one"),
            PathBuf::from("/home/tester/.ssh/two"),
        ]
    );
    assert_eq!(app.ignored_options, ["Compression"]);
    let other = &found.hosts[1];
    assert_eq!(other.user.as_deref(), Some("third"));
    assert_eq!(other.address(), "app.example");
}

#[test]
fn the_address_falls_back_to_the_alias() {
    let found = parse("Host plain\n");
    assert_eq!(found.hosts[0].address(), "plain");
}

#[test]
fn options_before_the_first_host_warn_and_apply_to_nobody() {
    let found = parse("User everyone\nForwardAgent yes\nHost a\n");
    assert_eq!(found.hosts[0].user, None);
    let warnings = vec![
        ImportWarning {
            source: virtual_line(1),
            kind: WarningKind::OptionOutsideHost {
                keyword: "User".to_owned(),
            },
        },
        ImportWarning {
            source: virtual_line(2),
            kind: WarningKind::OptionOutsideHost {
                keyword: "ForwardAgent".to_owned(),
            },
        },
    ];
    assert_eq!(found.warnings, warnings);
}

#[test]
fn bad_values_warn_and_leave_the_option_unset() {
    let found = parse("Host a\nPort 0\nPort ssh\nPort 70000\nUser\nHost\n");
    assert_eq!(found.hosts[0].port, None);
    assert_eq!(found.hosts[0].user, None);
    let kinds: Vec<WarningKind> = found.warnings.into_iter().map(|w| w.kind).collect();
    let invalid = |value: &str| WarningKind::InvalidPort {
        value: value.to_owned(),
    };
    let missing = |keyword: &str| WarningKind::MissingArgument {
        keyword: keyword.to_owned(),
    };
    assert_eq!(
        kinds,
        [
            invalid("0"),
            invalid("ssh"),
            invalid("70000"),
            missing("User"),
            missing("Host"),
        ]
    );
}

#[test]
fn proxy_jump_none_is_no_jump_and_still_wins() {
    let found =
        parse("Host a\nProxyJump none\nHost a\nProxyJump bastion\nHost b\nProxyJump j1,j2\n");
    assert_eq!(found.hosts[0].proxy_jump, None);
    assert_eq!(found.hosts[1].proxy_jump.as_deref(), Some("j1,j2"));
}

#[test]
fn identity_files_stay_paths_and_unknown_tokens_warn() {
    let found = parse(concat!(
        "Host k\n",
        "  User admin\n",
        "  IdentityFile /does/not/exist/%n_%u_%r_%p_%k\n",
        "  IdentityFile ~/.ssh/%C\n",
        "  IdentityFile C:\\Users\\tester\\.ssh\\id_rsa\n",
    ));
    assert_eq!(
        found.hosts[0].identity_files,
        [
            PathBuf::from("/does/not/exist/k_tester_admin_22_k"),
            PathBuf::from("/home/tester/.ssh/%C"),
            PathBuf::from("C:\\Users\\tester\\.ssh\\id_rsa"),
        ]
    );
    let warnings = vec![ImportWarning {
        source: virtual_line(4),
        kind: WarningKind::UnknownToken {
            token: "%C".to_owned(),
        },
    }];
    assert_eq!(found.warnings, warnings);
}

#[test]
fn crlf_line_ends_and_a_byte_order_mark_are_read() {
    let found = parse("\u{feff}Host win\r\n  HostName win.example\r\n  User me\r\n");
    assert_eq!(found.hosts[0].alias, "win");
    assert_eq!(found.hosts[0].address(), "win.example");
    assert_eq!(found.hosts[0].user.as_deref(), Some("me"));
    assert!(found.warnings.is_empty());
}

#[test]
fn a_broken_line_warns_and_reading_goes_on() {
    let found = parse("Host a\nUser \"open\n= x\nPort 2022\n");
    assert_eq!(found.hosts[0].port, Some(2022));
    let kinds: Vec<WarningKind> = found.warnings.into_iter().map(|w| w.kind).collect();
    assert_eq!(
        kinds,
        [WarningKind::UnterminatedQuote, WarningKind::MissingKeyword]
    );
}

#[test]
fn a_missing_root_file_is_an_error() {
    let missing = fixtures().join("no-such-config");
    let error = load(&missing, &context(&fixtures())).expect_err("nothing to read");
    assert!(
        matches!(&error, LoadError::Io(source) if source.kind() == io::ErrorKind::NotFound),
        "{error:?}"
    );
}

#[test]
fn the_context_points_at_the_users_ssh_directory() {
    let context = SshConfigContext::new("/home/tester", "tester");
    assert_eq!(context.ssh_dir, PathBuf::from("/home/tester/.ssh"));
    assert_eq!(
        context.user_config(),
        PathBuf::from("/home/tester/.ssh/config")
    );
}

#[test]
fn every_warning_has_its_own_code() {
    let file = PathBuf::from("f");
    let keyword = String::new();
    let kinds = [
        WarningKind::IncludeCycle { file: file.clone() },
        WarningKind::IncludeTooDeep { file: file.clone() },
        WarningKind::IncludeNotFound { file: file.clone() },
        WarningKind::IncludeUnreadable {
            file,
            error: io::ErrorKind::PermissionDenied,
        },
        WarningKind::IncludeInMatch,
        WarningKind::IncludeRefused {
            file: PathBuf::new(),
        },
        WarningKind::MatchSkipped,
        WarningKind::OptionOutsideHost {
            keyword: keyword.clone(),
        },
        WarningKind::MissingArgument { keyword },
        WarningKind::InvalidPort {
            value: String::new(),
        },
        WarningKind::UnknownToken {
            token: String::new(),
        },
        WarningKind::UnterminatedQuote,
        WarningKind::MissingKeyword,
        WarningKind::IncludeRepeated {
            file: PathBuf::new(),
        },
        WarningKind::TooManyIncludes,
        WarningKind::TooManyHosts,
    ];
    let codes: Vec<&str> = kinds.iter().map(WarningKind::code).collect();
    assert_eq!(
        codes,
        WarningKind::ALL_CODES,
        "ALL_CODES lists every code in order"
    );
    let mut unique = codes.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(unique.len(), kinds.len());
}

#[test]
fn every_skip_reason_is_in_its_code_list() {
    let codes: Vec<&str> = [SkipReason::Pattern, SkipReason::Negated]
        .into_iter()
        .map(SkipReason::code)
        .collect();
    assert_eq!(codes, SkipReason::ALL_CODES);
}
