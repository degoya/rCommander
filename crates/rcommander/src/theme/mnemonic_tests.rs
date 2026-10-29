use std::collections::BTreeMap;

use iced::{
    event::Event,
    keyboard::{
        self, Key, Location, Modifiers,
        key::{Code, Named, NativeCode, Physical},
    },
};

use super::{Label, pressed, shown_when};

#[test]
fn a_mark_names_the_letter_and_is_not_drawn() {
    let label = Label::parse("&Overwrite");
    assert_eq!(label.text, "Overwrite");
    assert_eq!(label.letter, Some('o'));
    assert_eq!(label.split(), Some(("", "O", "verwrite")));
    let label = Label::parse("S&kip all");
    assert_eq!(label.letter, Some('k'));
    assert_eq!(label.split(), Some(("S", "k", "ip all")));
    assert_eq!(Label::parse("Level &1").letter, Some('1'));
    let label = Label::parse("&\u{dc}ber");
    assert_eq!(label.letter, Some('\u{fc}'));
    assert_eq!(label.split(), Some(("", "\u{dc}", "ber")));
}

#[test]
fn ampersands_without_a_letter_stay_and_a_second_mark_goes() {
    for (label, text) in [
        ("Save && close", "Save & close"),
        ("Save & close", "Save & close"),
        ("Rock&", "Rock&"),
        ("No mark", "No mark"),
    ] {
        let parsed = Label::parse(label);
        assert_eq!(parsed.text, text, "{label}");
        assert_eq!(parsed.letter, None, "{label}");
        assert_eq!(parsed.split(), None, "{label}");
    }
    let twice = Label::parse("&Fish && &Chips");
    assert_eq!(twice.text, "Fish & Chips");
    assert_eq!(twice.letter, Some('f'));
}

#[test]
fn the_underline_shows_while_alt_is_held_or_always() {
    assert!(!shown_when(false, false));
    assert!(shown_when(false, true));
    assert!(shown_when(true, false));
}

fn key(key: Key, physical: Physical, modifiers: Modifiers) -> Event {
    Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: physical,
        location: Location::Standard,
        modifiers,
        text: None,
        repeat: false,
    })
}

fn letter(typed: &str, modifiers: Modifiers) -> Event {
    let unknown = Physical::Unidentified(NativeCode::Unidentified);
    key(Key::Character(typed.into()), unknown, modifiers)
}

#[test]
fn a_letter_or_digit_with_alt_or_shift_is_an_access_key_and_ctrl_or_altgr_is_not() {
    assert_eq!(pressed(&letter("o", Modifiers::ALT)), Some(('o', true)));
    assert_eq!(pressed(&letter("O", Modifiers::SHIFT)), Some(('o', false)));
    assert_eq!(
        pressed(&letter("3", Modifiers::empty())),
        Some(('3', false))
    );
    assert_eq!(pressed(&letter("o", Modifiers::CTRL)), None);
    assert_eq!(
        pressed(&letter("o", Modifiers::CTRL | Modifiers::ALT)),
        None
    );
    assert_eq!(pressed(&letter("o", Modifiers::LOGO)), None);
    assert_eq!(pressed(&letter("-", Modifiers::ALT)), None);
    let enter = key(
        Key::Named(Named::Enter),
        Physical::Code(Code::Enter),
        Modifiers::ALT,
    );
    assert_eq!(pressed(&enter), None);
    // A Cyrillic layout: the key where a Latin keyboard has O.
    let cyrillic = key(
        Key::Character("\u{449}".into()),
        Physical::Code(Code::KeyO),
        Modifiers::ALT,
    );
    assert_eq!(pressed(&cyrillic), Some(('o', true)));
}

/// Every text of the catalogue marks one access key at most, before a letter or a digit: the
/// one a button answers to.
#[test]
fn the_catalogue_marks_one_letter_at_most() {
    let catalogue: BTreeMap<String, String> =
        serde_json::from_str(include_str!("../../locales/en.json")).expect("the catalogue");
    for (key, value) in &catalogue {
        let mut marks = 0;
        let mut chars = value.chars();
        while let Some(c) = chars.next() {
            if c == '&' && chars.next().is_some_and(char::is_alphanumeric) {
                marks += 1;
            }
        }
        assert!(marks <= 1, "{key}: {value}");
        assert_eq!(Label::parse(value).letter.is_some(), marks == 1, "{key}");
    }
}
