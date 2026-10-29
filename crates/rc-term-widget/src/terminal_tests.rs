use alacritty_terminal::{event::Event as TermUiEvent, term::ClipboardType};
use iced::keyboard::{Key, Modifiers, key::Named};
use rc_term::{TermEvent, TermId};

use super::{Event, ReservedKey, burst};

#[test]
fn a_program_copy_to_the_clipboard_reaches_the_app_and_the_end_ends() {
    let id = TermId(7);
    let mut events = Vec::new();
    let copy = TermUiEvent::ClipboardStore(ClipboardType::Clipboard, "picked".into());
    assert!(!burst(id, TermEvent::Ui(copy), &mut events));
    let selection = TermUiEvent::ClipboardStore(ClipboardType::Selection, "primary".into());
    assert!(!burst(id, TermEvent::Ui(selection), &mut events));
    assert!(!burst(id, TermEvent::Wakeup, &mut events));
    assert!(
        matches!(&events[..], [Event::Copy(copied, text)] if *copied == id && text == "picked")
    );
    assert!(burst(id, TermEvent::Exited(Some(0)), &mut events));
}

/// The toggle on a German layout: the key gives an O umlaut, upper case with Shift.
#[test]
fn a_reserved_key_matches_its_character_and_modifiers_only() {
    let toggle = ReservedKey {
        character: "\u{f6}".to_owned(),
        modifiers: Modifiers::CTRL,
    };
    assert!(toggle.matches(&Key::Character("\u{f6}".into()), Modifiers::CTRL));
    assert!(toggle.matches(&Key::Character("\u{d6}".into()), Modifiers::CTRL));
    assert!(!toggle.matches(
        &Key::Character("\u{f6}".into()),
        Modifiers::CTRL | Modifiers::SHIFT
    ));
    assert!(!toggle.matches(&Key::Character("\u{f6}".into()), Modifiers::empty()));
    assert!(!toggle.matches(&Key::Character("^".into()), Modifiers::CTRL));
    assert!(!toggle.matches(&Key::Named(Named::Tab), Modifiers::CTRL));
}
