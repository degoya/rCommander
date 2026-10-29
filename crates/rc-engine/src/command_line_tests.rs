use super::*;

#[test]
fn cd_with_a_folder_moves_the_panel() {
    assert_eq!(parse("cd docs"), Parsed::Cd("docs".into()));
    assert_eq!(parse("  CD   docs/old  "), Parsed::Cd("docs/old".into()));
    assert_eq!(parse("chdir src"), Parsed::Cd("src".into()));
    assert_eq!(parse("cd .."), Parsed::Cd("..".into()));
    assert_eq!(parse("cd ..\\.."), Parsed::Cd("..\\..".into()));
    assert_eq!(parse("cd /tmp/x"), Parsed::Cd("/tmp/x".into()));
    assert_eq!(parse("cd C:\\Users"), Parsed::Cd("C:\\Users".into()));
}

#[test]
fn cmd_spellings_of_cd_are_understood() {
    assert_eq!(parse("cd.."), Parsed::Cd("..".into()));
    assert_eq!(parse("cd\\"), Parsed::Cd(native("\\")));
    assert_eq!(
        parse("cd /d \"C:\\Program Files\""),
        Parsed::Cd("C:\\Program Files".into())
    );
    assert_eq!(parse("cd /D d:\\work"), Parsed::Cd("d:\\work".into()));
    assert_eq!(parse("cd \"my docs\""), Parsed::Cd("my docs".into()));
    assert_eq!(
        parse("cd my docs"),
        Parsed::Cd("my docs".into()),
        "as cmd takes it"
    );
}

#[test]
fn cd_alone_stays_and_the_root_is_the_root() {
    assert_eq!(parse("cd"), Parsed::Stay);
    assert_eq!(parse("cd   "), Parsed::Stay);
    assert_eq!(parse("cd /d"), Parsed::Stay);
    let root = if cfg!(windows) { "\\" } else { "/" };
    assert_eq!(parse("cd \\"), Parsed::Cd(root.into()));
}

#[test]
fn a_drive_alone_changes_the_drive() {
    assert_eq!(parse("d:"), Parsed::Drive('D'));
    assert_eq!(parse("  C:  "), Parsed::Drive('C'));
    assert_eq!(parse("cd e:"), Parsed::Drive('E'));
    assert_eq!(parse("cd /d e:"), Parsed::Drive('E'));
    assert_eq!(parse("1:"), Parsed::Run("1:"), "no drive");
    assert_eq!(parse("d:\\x"), Parsed::Run("d:\\x"), "a path, not a drive");
}

#[test]
fn everything_else_runs_in_the_shell() {
    assert_eq!(parse("dir /w"), Parsed::Run("dir /w"));
    assert_eq!(parse("ls -la"), Parsed::Run("ls -la"));
    assert_eq!(parse("cdrom-tool"), Parsed::Run("cdrom-tool"));
    assert_eq!(parse("chdirs"), Parsed::Run("chdirs"));
    assert_eq!(parse("cd docs && dir"), Parsed::Run("cd docs && dir"));
    assert_eq!(parse("cd > out.txt"), Parsed::Run("cd > out.txt"));
}

#[test]
fn a_line_is_checked_at_the_boundary_without_repeating_it() {
    assert_eq!(check("  dir  ").expect("runs"), "dir");
    for (line, reason) in [
        ("", "empty"),
        ("   ", "empty"),
        ("dir\nformat c:", "control characters"),
        ("echo \u{0}", "control characters"),
    ] {
        match check(line) {
            Err(EngineError::CommandLineInvalid { reason: got }) => assert_eq!(got, reason),
            other => panic!("{line:?}: {other:?}"),
        }
    }
    let long = "x".repeat(MAX_LINE + 1);
    let error = check(&long).expect_err("too long");
    assert_eq!(error.code(), "command_line_invalid");
    assert!(
        !error.to_string().contains("xxxx"),
        "the line is not repeated"
    );
    assert!(check(&"x".repeat(MAX_LINE)).is_ok());
}

#[test]
fn cmd_quotes_names_with_blanks_or_special_characters() {
    assert_eq!(quote_for("notes.txt", Shell::Cmd), "notes.txt");
    assert_eq!(quote_for("my notes.txt", Shell::Cmd), "\"my notes.txt\"");
    assert_eq!(quote_for("a&b.txt", Shell::Cmd), "\"a&b.txt\"");
    assert_eq!(
        quote_for("C:\\Program Files\\x", Shell::Cmd),
        "\"C:\\Program Files\\x\""
    );
    assert_eq!(quote_for("C:\\work\\a.txt", Shell::Cmd), "C:\\work\\a.txt");
    assert_eq!(quote_for("", Shell::Cmd), "\"\"");
}

#[test]
fn sh_quotes_everything_but_plain_names() {
    assert_eq!(quote_for("notes.txt", Shell::Posix), "notes.txt");
    assert_eq!(
        quote_for("/home/me/a-b_c.txt", Shell::Posix),
        "/home/me/a-b_c.txt"
    );
    assert_eq!(quote_for("my notes.txt", Shell::Posix), "'my notes.txt'");
    assert_eq!(quote_for("it's.txt", Shell::Posix), "'it'\\''s.txt'");
    assert_eq!(quote_for("$HOME", Shell::Posix), "'$HOME'");
    assert_eq!(quote_for("", Shell::Posix), "''");
}

#[test]
fn only_a_local_folder_runs_a_command() {
    let local = VfsPath::local("/work");
    assert!(runs_in(&local));
    let inside = VfsPath::archive(local, rc_vfs::ArchiveFormat::Zip, "/docs");
    assert!(!runs_in(&inside));
}
