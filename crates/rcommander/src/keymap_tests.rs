use iced::keyboard::{
    Key, Modifiers,
    key::{Code, Named, NativeCode, Physical},
};
use rc_engine::{Chord, Command, Keymap, PanelView};

use super::{chord, pressed};

const ANY_KEY: Physical = Physical::Unidentified(NativeCode::Unidentified);

fn spelled(key: Key, physical: Physical, modifiers: Modifiers) -> Option<String> {
    chord(&key, physical, modifiers).map(|chord| chord.to_string())
}

#[test]
fn named_keys_keep_their_names() {
    for (named, text) in [
        (Named::Tab, "Tab"),
        (Named::Enter, "Enter"),
        (Named::Backspace, "Backspace"),
        (Named::ArrowDown, "ArrowDown"),
        (Named::PageUp, "PageUp"),
        (Named::Insert, "Insert"),
        (Named::Space, "Space"),
        (Named::F5, "F5"),
        (Named::ContextMenu, "ContextMenu"),
    ] {
        assert_eq!(
            spelled(Key::Named(named), ANY_KEY, Modifiers::empty()).as_deref(),
            Some(text)
        );
    }
}

#[test]
fn modifiers_and_characters_are_spelled_like_the_keymap() {
    assert_eq!(
        spelled(
            Key::Character("F".into()),
            ANY_KEY,
            Modifiers::CTRL | Modifiers::SHIFT
        ),
        Some("Ctrl+Shift+F".to_owned())
    );
    assert_eq!(
        chord(&Key::Character("a".into()), ANY_KEY, Modifiers::ALT),
        "Alt+A".parse::<Chord>().ok()
    );
}

/// The toggle as the owner wants it (2026-10-17, as VS Code): Ctrl+O-umlaut on a German
/// layout, Ctrl+` on a US one - the same virtual key `VK_OEM_3`, sitting where US has `;` and
/// left of 1 respectively. The German key left of 1 (`^`) is no toggle.
#[test]
fn the_toggle_is_ctrl_oe_on_german_and_ctrl_backquote_on_us() {
    let keymap = Keymap::standard();
    let toggle = Some(Command::ToggleTerminal { panel: None });
    let german_oe = chord(
        &Key::Character("\u{f6}".into()),
        Physical::Code(Code::Semicolon),
        Modifiers::CTRL,
    );
    let us_backquote = chord(
        &Key::Character("`".into()),
        Physical::Code(Code::Backquote),
        Modifiers::CTRL,
    );
    for pressed in [german_oe, us_backquote] {
        let command = pressed
            .clone()
            .and_then(|chord| keymap.lookup(&chord, PanelView::Terminal).cloned());
        assert_eq!(command, toggle, "{pressed:?}");
    }
    let german_circumflex = chord(
        &Key::Character("^".into()),
        Physical::Code(Code::Backquote),
        Modifiers::CTRL,
    );
    let command =
        german_circumflex.and_then(|chord| keymap.lookup(&chord, PanelView::Terminal).cloned());
    assert_eq!(command, None, "Ctrl+^ goes to the shell");
    let new_terminal = chord(
        &Key::Character("\u{f6}".into()),
        Physical::Code(Code::Semicolon),
        Modifiers::CTRL | Modifiers::SHIFT,
    );
    assert_eq!(new_terminal, "Ctrl+Shift+\u{d6}".parse::<Chord>().ok());
}

/// The terminal over the whole window (RC-040-05): Ctrl+A-umlaut on a German layout, Ctrl+' on
/// a US one - the same virtual key `VK_OEM_7`, right of the toggle's key on both.
#[test]
fn maximize_is_ctrl_ae_on_german_and_ctrl_quote_on_us() {
    let keymap = Keymap::standard();
    let maximize = Some(Command::MaximizeTerminal { panel: None });
    for character in ["\u{e4}", "'"] {
        let pressed = chord(
            &Key::Character(character.into()),
            Physical::Code(Code::Quote),
            Modifiers::CTRL,
        );
        for view in [PanelView::Files, PanelView::Terminal] {
            let command = pressed
                .clone()
                .and_then(|chord| keymap.lookup(&chord, view).cloned());
            assert_eq!(command, maximize, "{pressed:?} in {view:?}");
        }
    }
}

#[test]
fn a_bare_modifier_or_the_windows_key_is_no_chord() {
    assert_eq!(
        chord(&Key::Named(Named::Shift), ANY_KEY, Modifiers::SHIFT),
        None
    );
    assert_eq!(
        chord(&Key::Character("e".into()), ANY_KEY, Modifiers::LOGO),
        None
    );
    assert_eq!(chord(&Key::Unidentified, ANY_KEY, Modifiers::empty()), None);
}

/// Total Commander's Ctrl+\ is Ctrl+# on a German keyboard: the same key.
#[test]
fn the_backslash_key_and_the_keypad_are_named_by_position() {
    let backslash = Physical::Code(Code::Backslash);
    for logical in ["\\", "#"] {
        assert_eq!(
            spelled(Key::Character(logical.into()), backslash, Modifiers::CTRL).as_deref(),
            Some("Ctrl+\\"),
            "{logical}"
        );
    }
    for (code, logical, name) in [
        (Code::NumpadAdd, "+", "NumAdd"),
        (Code::NumpadSubtract, "-", "NumSubtract"),
        (Code::NumpadMultiply, "*", "NumMultiply"),
        (Code::NumpadDivide, "/", "NumDivide"),
    ] {
        assert_eq!(
            spelled(
                Key::Character(logical.into()),
                Physical::Code(code),
                Modifiers::empty()
            )
            .as_deref(),
            Some(name)
        );
    }
    assert_eq!(
        spelled(Key::Character("+".into()), ANY_KEY, Modifiers::CTRL).as_deref(),
        Some("Ctrl+Plus"),
        "the + of the main block is no keypad key, and `+` joins a chord (RC-020-12)"
    );
}

/// AltGr as winit reports it on a German layout: neither Ctrl nor Alt, and the key with its
/// modifiers unidentified when AltGr gives it no character (RC-010-08, Windows test 2026-10-18).
#[test]
fn altgr_with_a_letter_it_types_nothing_for_is_ctrl_alt() {
    let f = Key::Character("f".into());
    let key_f = Physical::Code(Code::KeyF);
    let ctrl_alt_f = "Ctrl+Alt+f".parse::<Chord>().ok();
    assert_eq!(
        pressed(&f, &Key::Unidentified, key_f, Modifiers::empty()),
        ctrl_alt_f
    );
    assert_eq!(
        pressed(
            &Key::Character("1".into()),
            &Key::Unidentified,
            ANY_KEY,
            Modifiers::empty()
        ),
        "Ctrl+Alt+1".parse::<Chord>().ok()
    );
    // Ctrl and Alt on their own keys come as they are.
    assert_eq!(
        pressed(
            &f,
            &Key::Unidentified,
            key_f,
            Modifiers::CTRL | Modifiers::ALT
        ),
        ctrl_alt_f
    );
    // AltGr+Q types `@`: the chord stays plain, the character is typed.
    assert_eq!(
        pressed(
            &Key::Character("q".into()),
            &Key::Character("@".into()),
            Physical::Code(Code::KeyQ),
            Modifiers::empty()
        ),
        "q".parse::<Chord>().ok()
    );
    // A plain letter, and a dead key (whose modified key is unidentified too), stay plain.
    assert_eq!(
        pressed(&f, &f, key_f, Modifiers::empty()),
        "f".parse::<Chord>().ok()
    );
    assert_eq!(
        pressed(
            &Key::Character("^".into()),
            &Key::Unidentified,
            Physical::Code(Code::Backquote),
            Modifiers::empty()
        ),
        "^".parse::<Chord>().ok()
    );
}
