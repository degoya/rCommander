use std::path::PathBuf;

use rc_term::{Platform, ShellEnv, ShellProfile, detect_shells};

use super::{reserved_keys, shell_label, shells};

fn profile(id: &str, label_key: &str) -> ShellProfile {
    ShellProfile {
        id: id.to_owned(),
        label_key: label_key.to_owned(),
        program: PathBuf::from(id),
        args: Vec::new(),
        distro: None,
    }
}

#[test]
fn the_windows_shells_have_names() {
    let shells = detect_shells(Platform::Windows, &ShellEnv::default());
    let labels: Vec<String> = shells.iter().map(shell_label).collect();
    assert_eq!(labels, ["Windows PowerShell", "Command Prompt"]);
}

#[test]
fn a_shell_the_catalogue_does_not_name_shows_its_id() {
    assert_eq!(
        shell_label(&profile("pwsh", "terminal.shell.pwsh")),
        "PowerShell 7"
    );
    assert_eq!(
        shell_label(&profile("nu", "terminal.shell.other")),
        "nu",
        "an unknown login shell"
    );
    assert_eq!(
        shell_label(&profile("ssh-box", "terminal.shell.ssh")),
        "ssh-box",
        "a profile of a later back end"
    );
    let debian = ShellProfile {
        distro: Some("Debian".to_owned()),
        ..profile("wsl", "terminal.shell.wsl")
    };
    assert_eq!(
        shell_label(&debian),
        "WSL: Debian",
        "one profile per distribution"
    );
}

#[test]
fn there_is_always_a_shell_to_offer() {
    assert!(!shells().is_empty());
}

/// The chords a terminal leaves to the app are the toggle's, Ctrl+O-umlaut (German layout) and
/// Ctrl+` (US layout), the maximize's (RC-040-05), Ctrl+A-umlaut and Ctrl+', and the list of the
/// sessions (RC-080-16), Ctrl+Shift+A; nothing else.
#[test]
fn the_reserved_keys_are_the_toggle_the_maximize_and_the_session_list() {
    use iced::keyboard::Modifiers;
    use rc_engine::Keymap;
    use rc_term_widget::ReservedKey;

    let reserved = reserved_keys(&Keymap::standard());
    let toggle = |character: &str| ReservedKey {
        character: character.to_owned(),
        modifiers: Modifiers::CTRL,
    };
    let list = ReservedKey {
        character: "a".to_owned(),
        modifiers: Modifiers::CTRL | Modifiers::SHIFT,
    };
    // The image pasted as a path (RC-040-08).
    let image = ReservedKey {
        character: "v".to_owned(),
        modifiers: Modifiers::CTRL | Modifiers::ALT,
    };
    assert_eq!(
        reserved,
        [
            toggle("\u{f6}"),
            toggle("`"),
            toggle("\u{e4}"),
            toggle("'"),
            image,
            list
        ]
    );
}
