use rc_engine::Chord;

use super::{key_message, typed_text};
use crate::app::{Message, cmdline::CmdLineMsg};

fn chord(keys: &str) -> Chord {
    keys.parse().expect("a chord")
}

#[test]
fn a_printable_key_without_ctrl_or_alt_types_its_character() {
    assert_eq!(typed_text(&chord("a"), Some("a")), Some("a".into()));
    assert_eq!(typed_text(&chord("Shift+a"), Some("A")), Some("A".into()));
    assert_eq!(typed_text(&chord("Space"), Some(" ")), Some(" ".into()));
    assert_eq!(typed_text(&chord("NumAdd"), Some("+")), Some("+".into()));
}

#[test]
fn control_characters_and_shortcuts_type_nothing() {
    assert_eq!(typed_text(&chord("Enter"), Some("\r")), None);
    assert_eq!(typed_text(&chord("Backspace"), Some("\u{8}")), None);
    assert_eq!(typed_text(&chord("Ctrl+a"), Some("\u{1}")), None);
    assert_eq!(typed_text(&chord("Ctrl+p"), Some("p")), None);
    assert_eq!(typed_text(&chord("Alt+x"), Some("x")), None);
    assert_eq!(typed_text(&chord("F5"), None), None);
}

/// AltGr is Ctrl+Alt on Windows: `@` from AltGr+Q is text, Ctrl+Alt+Q is the quick search.
#[test]
fn altgr_types_but_ctrl_alt_with_the_keys_own_letter_does_not() {
    assert_eq!(
        typed_text(&chord("Ctrl+Alt+q"), Some("@")),
        Some("@".into())
    );
    assert_eq!(typed_text(&chord("Ctrl+Alt+q"), Some("q")), None);
    assert_eq!(typed_text(&chord("Ctrl+Alt+q"), None), None);
}

#[test]
fn a_typed_key_is_a_typed_message_and_any_other_a_key() {
    assert!(matches!(
        key_message(chord("d"), Some("d")),
        Message::CmdLine(CmdLineMsg::Typed(_, text)) if text == "d"
    ));
    assert!(matches!(
        key_message(chord("Ctrl+Alt+f"), None),
        Message::Key(key) if key == chord("Ctrl+Alt+f")
    ));
}
