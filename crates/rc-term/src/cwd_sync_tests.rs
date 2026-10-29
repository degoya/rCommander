//! The rules of typing into a shell, on the state alone; `session_tests.rs` runs them through a
//! session with a fake program.

use std::path::{Path, PathBuf};

use super::{Changes, CwdSync, FollowOutcome, FollowStatus, PromptState};
use crate::{Platform, ShellPaths, ShellSyntax};

const PROMPT: &[u8] = b"133;A";
const INPUT: &[u8] = b"133;B";
const RUNNING: &[u8] = b"133;C";

fn bash() -> CwdSync {
    let mut sync = CwdSync::new(PathBuf::from("/start"), vec!["host".to_owned()]);
    sync.configure(Some(ShellSyntax::Posix), ShellPaths::Native(Platform::Unix));
    sync
}

fn mark(sync: &mut CwdSync, payload: &[u8]) -> Changes {
    let mut changes = Changes::default();
    sync.on_payload(payload, &mut changes);
    changes
}

fn sent(line: Option<Vec<u8>>) -> String {
    String::from_utf8(line.expect("a cd")).expect("utf-8")
}

#[test]
fn without_prompt_marks_nothing_is_ever_typed() {
    let mut sync = bash();
    let (outcome, line) = sync.follow(Path::new("/elsewhere"));
    assert_eq!(outcome, FollowOutcome::Waiting(FollowStatus::NoIntegration));
    assert!(line.is_none());
    sync.on_input(b"ls\r");
    let changes = mark(&mut sync, b"7;file:///start");
    assert!(changes.write.is_none());
    assert_eq!(sync.prompt(), PromptState::Unknown);
    assert_eq!(sync.status(), Some(FollowStatus::NoIntegration));
}

#[test]
fn at_an_empty_prompt_the_cd_is_typed_at_once() {
    let mut sync = bash();
    mark(&mut sync, PROMPT);
    mark(&mut sync, INPUT);
    let (outcome, line) = sync.follow(Path::new("/it's here"));
    assert_eq!(outcome, FollowOutcome::Sent);
    assert_eq!(sent(line), " cd -- '/it'\\''s here'\r");
    assert_eq!(sync.prompt(), PromptState::Running, "the prompt is left");
    assert_eq!(sync.current(), PathBuf::from("/it's here"));
}

#[test]
fn while_a_program_runs_the_cd_waits_for_the_next_prompt() {
    let mut sync = bash();
    mark(&mut sync, PROMPT);
    sync.on_input(b"vim notes.txt\r");
    assert_eq!(sync.prompt(), PromptState::Running);
    let (outcome, line) = sync.follow(Path::new("/elsewhere"));
    assert_eq!(outcome, FollowOutcome::Waiting(FollowStatus::Running));
    assert!(line.is_none());
    // A program that writes a 133;C or a folder changes nothing about that.
    assert!(mark(&mut sync, RUNNING).write.is_none());
    assert!(mark(&mut sync, b"7;file:///start").write.is_none());
    let changes = mark(&mut sync, PROMPT);
    assert_eq!(sent(changes.write), " cd -- '/elsewhere'\r");
    assert!(changes.prompt);
    assert_eq!(sync.status(), None);
    // Once only.
    assert!(mark(&mut sync, INPUT).write.is_none());
}

#[test]
fn a_typed_line_is_never_completed_by_the_cd() {
    let mut sync = bash();
    mark(&mut sync, PROMPT);
    sync.on_input(b"rm -rf ");
    let (outcome, _) = sync.follow(Path::new("/elsewhere"));
    assert_eq!(outcome, FollowOutcome::Waiting(FollowStatus::LineNotEmpty));
    // Typeahead stays on the line when the next prompt comes: still nothing.
    assert!(mark(&mut sync, PROMPT).write.is_none());
    sync.on_input(b"\x03");
    assert!(mark(&mut sync, PROMPT).write.is_some(), "Ctrl+C emptied it");
}

#[test]
fn the_reported_folder_is_read_and_a_cd_there_is_not_needed() {
    let mut sync = bash();
    mark(&mut sync, PROMPT);
    let changes = mark(&mut sync, b"7;file://host/home/my%20dir");
    assert!(changes.folder);
    assert_eq!(sync.reported(), Some(PathBuf::from("/home/my dir")));
    assert!(
        !mark(&mut sync, b"7;file://host/home/my%20dir").folder,
        "no change"
    );
    assert_eq!(
        sync.follow(Path::new("/home/my dir/")).0,
        FollowOutcome::Already
    );
    // Another host's folder is not this machine's.
    assert!(!mark(&mut sync, b"7;file://server/srv").folder);
}

/// A WSL shell follows the panel only inside its own distribution's share; a Windows folder, a
/// network share or another distribution's share is another system (owner's Windows test,
/// 2026-10-18). A shell the app does not type into is unreachable everywhere.
#[test]
fn a_wsl_shell_follows_only_inside_its_distribution() {
    let mut wsl = CwdSync::new(PathBuf::from(r"\\wsl.localhost\Ubuntu\home"), Vec::new());
    wsl.configure(
        Some(ShellSyntax::Posix),
        ShellPaths::Wsl(Some("Ubuntu".to_owned())),
    );
    mark(&mut wsl, PROMPT);
    for elsewhere in [
        r"\\server\share",
        r"C:\Users\x",
        r"\\wsl.localhost\Debian\home",
    ] {
        let (outcome, line) = wsl.follow(Path::new(elsewhere));
        assert_eq!(outcome, FollowOutcome::Unreachable, "{elsewhere}");
        assert!(line.is_none(), "{elsewhere}");
        assert_eq!(wsl.status(), Some(FollowStatus::OtherSystem), "{elsewhere}");
        assert!(!wsl.fits(Path::new(elsewhere)), "{elsewhere}");
    }
    let (outcome, line) = wsl.follow(Path::new(r"\\wsl$\ubuntu\srv\my dir"));
    assert_eq!(outcome, FollowOutcome::Sent);
    assert_eq!(sent(line), " cd -- '/srv/my dir'\r");
    assert_eq!(wsl.status(), None);

    let mut fish = CwdSync::new(PathBuf::from("/start"), Vec::new());
    mark(&mut fish, PROMPT);
    assert_eq!(
        fish.follow(Path::new("/elsewhere")).0,
        FollowOutcome::Unreachable
    );
    assert_eq!(fish.status(), Some(FollowStatus::Unreachable));
}

/// The screenshot of the owner's Windows test (2026-10-18): the panel went to
/// `\\wsl.localhost\debian\home` and PowerShell got `Set-Location` into the share. A shell of
/// this machine never follows into a WSL share; it stays and says why.
#[test]
fn a_local_shell_never_follows_into_a_wsl_share() {
    let mut pwsh = CwdSync::new(PathBuf::from(r"C:\___cloud___"), Vec::new());
    pwsh.configure(
        Some(ShellSyntax::PowerShell),
        ShellPaths::Native(Platform::Windows),
    );
    mark(&mut pwsh, PROMPT);
    for share in [r"\\wsl.localhost\debian\home", r"\\wsl$\Ubuntu"] {
        let (outcome, line) = pwsh.follow(Path::new(share));
        assert_eq!(outcome, FollowOutcome::Unreachable, "{share}");
        assert!(line.is_none(), "nothing typed: {share}");
        assert_eq!(pwsh.status(), Some(FollowStatus::OtherSystem));
        assert!(!pwsh.fits(Path::new(share)), "{share}");
    }
    assert!(pwsh.fits(Path::new(r"\\server\share")));
    assert!(pwsh.fits(Path::new(r"D:\work")));
    assert_eq!(pwsh.follow(Path::new(r"D:\work")).0, FollowOutcome::Sent);
}

/// A shell is where the panel is - a WSL shell that went to `/mnt/c/x` itself, the panel
/// following it to `C:\x` - fits there: the toggle shows it again rather than a new shell.
#[test]
fn a_shell_fits_the_folder_it_reported() {
    let mut wsl = CwdSync::new(PathBuf::from(r"\\wsl.localhost\Ubuntu\home"), Vec::new());
    wsl.configure(
        Some(ShellSyntax::Posix),
        ShellPaths::Wsl(Some("Ubuntu".to_owned())),
    );
    mark(&mut wsl, b"7;file:///mnt/c/x");
    assert_eq!(wsl.reported(), Some(PathBuf::from(r"C:\x")));
    assert!(wsl.fits(Path::new(r"C:\x")));
    assert!(!wsl.fits(Path::new(r"C:\y")));
}

#[test]
fn windows_folders_compare_ignoring_case() {
    let mut pwsh = CwdSync::new(PathBuf::from(r"C:\Users\X"), Vec::new());
    pwsh.configure(
        Some(ShellSyntax::PowerShell),
        ShellPaths::Native(Platform::Windows),
    );
    mark(&mut pwsh, PROMPT);
    assert_eq!(
        pwsh.follow(Path::new(r"c:\users\x\")).0,
        FollowOutcome::Already
    );
    mark(&mut pwsh, b"9;9;\"D:\\work\"");
    assert_eq!(pwsh.reported(), Some(PathBuf::from(r"D:\work")));
    let (outcome, line) = pwsh.follow(Path::new(r"C:\a'b"));
    assert_eq!(outcome, FollowOutcome::Sent);
    assert_eq!(sent(line), " Set-Location -LiteralPath 'C:\\a''b'\r");
}

#[test]
fn a_network_share_reported_by_a_shell_never_becomes_the_panel_folder() {
    // A Windows shell (or a server whose output it shows) names a share (RC-080-10).
    let mut cmd = CwdSync::new(PathBuf::from(r"C:\start"), Vec::new());
    cmd.configure(None, ShellPaths::Native(Platform::Windows));
    mark(&mut cmd, br"9;9;D:\work");
    assert_eq!(cmd.reported(), Some(PathBuf::from(r"D:\work")));
    for payload in [
        &br"9;9;\\attacker\share"[..],
        b"9;9;//attacker/share/x",
        b"7;file:////attacker/share",
        b"7;file:///%5C%5Cattacker%5Cshare",
    ] {
        let changes = mark(&mut cmd, payload);
        assert!(!changes.folder, "{}", String::from_utf8_lossy(payload));
        assert_eq!(cmd.reported(), Some(PathBuf::from(r"D:\work")));
    }
    // A WSL distribution's share is no network.
    mark(&mut cmd, br"9;9;\\wsl.localhost\Ubuntu\home\x");
    assert_eq!(
        cmd.reported(),
        Some(PathBuf::from(r"\\wsl.localhost\Ubuntu\home\x"))
    );
    let mut wsl = CwdSync::new(PathBuf::from("/"), Vec::new());
    wsl.configure(None, ShellPaths::Wsl(Some("Ubuntu".to_owned())));
    mark(&mut wsl, b"7;file:///home/x");
    assert_eq!(
        wsl.reported(),
        Some(PathBuf::from(r"\\wsl.localhost\Ubuntu\home\x"))
    );
}
