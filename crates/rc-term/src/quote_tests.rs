//! The quoting is checked two ways: the exact line, and - where the shell is installed - what
//! the shell itself makes of it (`posix_shell_reads_the_name_back`).

use super::{ShellSyntax, cd_line};

/// Names that would run something if a shell read them as code.
const HOSTILE: &[&str] = &[
    "it's here",
    "a b$HOME",
    "x; touch pwned",
    "x && touch pwned",
    "$(touch pwned)",
    "`touch pwned`",
    "a|b>c<d",
    "'; touch pwned; '",
    "-rf",
];

#[test]
fn posix_quotes_in_single_quotes() {
    let line = |path| cd_line(ShellSyntax::Posix, path);
    assert_eq!(line("/home/x").as_deref(), Some(" cd -- '/home/x'\r"));
    assert_eq!(
        line("/a b/it's $x").as_deref(),
        Some(" cd -- '/a b/it'\\''s $x'\r")
    );
}

#[test]
fn powershell_doubles_every_kind_of_single_quote() {
    let line = |path| cd_line(ShellSyntax::PowerShell, path);
    assert_eq!(
        line(r"C:\a b\it's $x").as_deref(),
        Some(" Set-Location -LiteralPath 'C:\\a b\\it''s $x'\r")
    );
    // PowerShell ends a single-quoted string at a typographic quote too.
    assert_eq!(
        line("C:\\x\u{2019}; calc").as_deref(),
        Some(" Set-Location -LiteralPath 'C:\\x\u{2019}\u{2019}; calc'\r")
    );
    // -LiteralPath: brackets are no wildcard.
    assert_eq!(
        line(r"C:\[a]").as_deref(),
        Some(" Set-Location -LiteralPath 'C:\\[a]'\r")
    );
}

#[test]
fn cmd_quotes_in_double_quotes_and_refuses_what_expands() {
    let line = |path| cd_line(ShellSyntax::Cmd, path);
    assert_eq!(
        line(r"C:\a & b ^ c").as_deref(),
        Some(" cd /d \"C:\\a & b ^ c\"\r")
    );
    assert_eq!(line(r"C:\%PATH%"), None);
    assert_eq!(line(r"C:\a!b"), None);
    assert_eq!(line("C:\\a\"b"), None);
}

#[test]
fn control_characters_and_empty_paths_get_no_line() {
    for syntax in [
        ShellSyntax::Posix,
        ShellSyntax::PowerShell,
        ShellSyntax::Cmd,
    ] {
        assert_eq!(cd_line(syntax, ""), None);
        assert_eq!(cd_line(syntax, "/a\nrm -rf ~"), None, "{syntax:?}");
        assert_eq!(cd_line(syntax, "/a\rb"), None);
        assert_eq!(cd_line(syntax, "/a\x1b[2Jb"), None);
    }
}

#[test]
fn every_hostile_name_stays_inside_its_quotes() {
    for name in HOSTILE {
        let posix = cd_line(ShellSyntax::Posix, name).expect("a line");
        let inner = posix
            .strip_prefix(" cd -- '")
            .and_then(|rest| rest.strip_suffix("'\r"))
            .expect("quoted");
        // Outside the quotes there is only the `'\''` of an embedded quote.
        assert_eq!(inner.replace(r"'\''", "'"), *name);
        let pwsh = cd_line(ShellSyntax::PowerShell, name).expect("a line");
        let inner = pwsh
            .strip_prefix(" Set-Location -LiteralPath '")
            .and_then(|rest| rest.strip_suffix("'\r"))
            .expect("quoted");
        assert!(!inner.replace("''", "").contains('\''), "{name}");
    }
}

#[test]
fn syntaxes_follow_the_shell_ids() {
    assert_eq!(ShellSyntax::of("bash"), Some(ShellSyntax::Posix));
    assert_eq!(ShellSyntax::of("wsl"), Some(ShellSyntax::Posix));
    assert_eq!(ShellSyntax::of("pwsh"), Some(ShellSyntax::PowerShell));
    assert_eq!(ShellSyntax::of("powershell"), Some(ShellSyntax::PowerShell));
    assert_eq!(ShellSyntax::of("cmd"), Some(ShellSyntax::Cmd));
    assert_eq!(ShellSyntax::of("fish"), None);
}

/// The real proof: `sh` itself runs the line in a fresh folder whose name is hostile, and must
/// land in it and create nothing.
#[cfg(unix)]
#[test]
fn posix_shell_reads_the_name_back() {
    use std::process::Command;

    use rc_process::NoConsoleWindow as _;

    let root = tempfile::tempdir().expect("temp dir");
    for name in HOSTILE {
        let folder = root.path().join(name.replace('/', "_"));
        std::fs::create_dir(&folder).expect("mkdir");
        let path = folder.to_str().expect("utf-8");
        let line = cd_line(ShellSyntax::Posix, path).expect("a line");
        let script = format!("{}\npwd\n", line.trim_end_matches('\r'));
        let output = Command::new("sh")
            .arg("-c")
            .arg(&script)
            .current_dir(root.path())
            .no_console_window()
            .output()
            .expect("sh runs");
        assert!(output.status.success(), "{name}: {output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), path);
        assert!(!root.path().join("pwned").exists(), "{name} ran something");
    }
}
