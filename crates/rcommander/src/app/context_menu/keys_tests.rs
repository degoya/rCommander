use super::*;

/// The keys come from the keymap's row of the command; a command without one shows none, and the
/// setting off hides every key (RC-010-24).
#[test]
fn an_entry_shows_its_commands_key_unless_the_keys_are_hidden() {
    let keymap = Keymap::standard();
    let close: Chord = "Ctrl+W".parse().expect("chord");
    assert_eq!(
        of_command(&keymap, "tabs.close"),
        Some(crate::keymap::label_text("Ctrl+W"))
    );
    assert_eq!(of_command(&keymap, "tabs.lock"), None, "no key of its own");
    assert_eq!(of_chord(&close), Some(crate::keymap::label_text("Ctrl+W")));
    assert_eq!(of_text(Some("Delete")), Some("Delete"));

    set_shown(false);
    assert_eq!(of_command(&keymap, "tabs.close"), None);
    assert_eq!(of_chord(&close), None);
    assert_eq!(of_text(Some("Delete")), None);
    set_shown(true);
    assert!(shown());
}
