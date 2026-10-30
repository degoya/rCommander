use alacritty_terminal::{event::Event as TermUiEvent, term::ClipboardType};
use iced::keyboard::{
    Key, Modifiers,
    key::{Code, Named, NativeCode, Physical},
};
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

/// A character key on a German layout: the key gives an O umlaut, upper case with Shift.
#[test]
fn a_reserved_key_matches_its_character_and_modifiers_only() {
    let anywhere = Physical::Unidentified(NativeCode::Unidentified);
    let toggle = ReservedKey {
        character: "\u{f6}".to_owned(),
        modifiers: Modifiers::CTRL,
    };
    let o_umlaut = Key::Character("\u{f6}".into());
    assert!(toggle.matches(&o_umlaut, anywhere, Modifiers::CTRL));
    let upper = Key::Character("\u{d6}".into());
    assert!(toggle.matches(&upper, anywhere, Modifiers::CTRL));
    assert!(!toggle.matches(&o_umlaut, anywhere, Modifiers::CTRL | Modifiers::SHIFT));
    assert!(!toggle.matches(&o_umlaut, anywhere, Modifiers::empty()));
    let circumflex = Key::Character("^".into());
    assert!(!toggle.matches(&circumflex, anywhere, Modifiers::CTRL));
    assert!(!toggle.matches(&Key::Named(Named::Tab), anywhere, Modifiers::CTRL));
}

/// A key named by place (RC-100-48) matches wherever it sits, whatever it prints - a dead key,
/// which gives no character, too.
#[test]
fn a_reserved_key_by_place_matches_its_place_on_every_layout() {
    let toggle = ReservedKey {
        character: "[Semicolon]".to_owned(),
        modifiers: Modifiers::CTRL,
    };
    let semicolon = Physical::Code(Code::Semicolon);
    for character in ["\u{f6}", ";", "\u{f1}"] {
        let key = Key::Character(character.into());
        assert!(
            toggle.matches(&key, semicolon, Modifiers::CTRL),
            "{character}"
        );
        assert!(!toggle.matches(&key, semicolon, Modifiers::empty()));
    }
    let o_umlaut = Key::Character("\u{f6}".into());
    assert!(!toggle.matches(&o_umlaut, Physical::Code(Code::Quote), Modifiers::CTRL));
    let acute = ReservedKey {
        character: "[Quote]".to_owned(),
        modifiers: Modifiers::CTRL,
    };
    let quote = Physical::Code(Code::Quote);
    assert!(acute.matches(&Key::Unidentified, quote, Modifiers::CTRL));
}
