//! RC-110-58: macOS's launchd folders and the programs Windows installs per user are no target
//! of an MCP write, written in any case and as Windows opens them; folders that merely share a
//! name with them stay writable.

use std::path::PathBuf;

use super::protected_target;

const AUTOSTART: &str = "an autostart folder";
const PROGRAMS: &str = "a folder of installed programs";

fn path(parts: &[&str]) -> PathBuf {
    parts.iter().collect()
}

#[test]
fn launchd_folders_are_autostart_folders() {
    for parts in [
        &["Users", "bob", "Library", "LaunchAgents"][..],
        &["Users", "bob", "Library", "LaunchAgents", "com.evil.plist"],
        &["Users", "bob", "Library", "LaunchDaemons", "x.plist"],
        &["Library", "LaunchAgents", "com.evil.plist"],
        &["Library", "LaunchDaemons", "com.evil.plist"],
        &["users", "bob", "library", "launchagents", "x.plist"],
    ] {
        assert_eq!(protected_target(&path(parts)), Some(AUTOSTART), "{parts:?}");
    }
    for parts in [
        &["Users", "bob", "Library", "Preferences", "x.plist"][..],
        &["Users", "bob", "LaunchAgents", "x.plist"],
        &["Users", "bob", "Library", "LaunchAgents-backup", "x.plist"],
        &["Users", "bob", "Documents", "library", "notes.txt"],
    ] {
        assert_eq!(protected_target(&path(parts)), None, "{parts:?}");
    }
}

#[test]
fn programs_installed_per_user_are_protected() {
    for parts in [
        &["Users", "Bob", "AppData", "Local", "Programs"][..],
        &[
            "Users",
            "Bob",
            "AppData",
            "Local",
            "Programs",
            "Microsoft VS Code",
            "version.dll",
        ],
        &["users", "bob", "appdata", "local", "programs", "x", "y.exe"],
        // Windows opens `Programs.` as `Programs`.
        &["Users", "Bob", "AppData", "Local", "Programs.", "x.dll"],
    ] {
        assert_eq!(protected_target(&path(parts)), Some(PROGRAMS), "{parts:?}");
    }
    for parts in [
        &["Users", "Bob", "AppData", "Local", "Temp", "x.dll"][..],
        &["Users", "Bob", "AppData", "Roaming", "Programs", "x.dll"],
        &["Users", "Bob", "Documents", "Programs", "x.dll"],
        &["Program Files", "Tool", "x.dll"],
    ] {
        assert_eq!(protected_target(&path(parts)), None, "{parts:?}");
    }
}
