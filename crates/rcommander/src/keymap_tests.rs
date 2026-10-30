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

/// The toggle on the key right of L (RC-100-48): Ctrl+O-umlaut on a German layout, Ctrl+; on a
/// US one; the key left of 1 by place as well (owner 2026-10-29): VS Code's Ctrl+` on US, the
/// circumflex on German - a dead key, which gives no character -, the superscript two on AZERTY.
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
    let us_semicolon = chord(
        &Key::Character(";".into()),
        Physical::Code(Code::Semicolon),
        Modifiers::CTRL,
    );
    for pressed in [german_oe, us_backquote, us_semicolon] {
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
    let german_dead = chord(
        &Key::Unidentified,
        Physical::Code(Code::Backquote),
        Modifiers::CTRL,
    );
    let azerty_two = chord(
        &Key::Character("\u{b2}".into()),
        Physical::Code(Code::Backquote),
        Modifiers::CTRL,
    );
    for pressed in [german_circumflex, german_dead, azerty_two] {
        let command = pressed
            .clone()
            .and_then(|chord| keymap.lookup(&chord, PanelView::Terminal).cloned());
        assert_eq!(command, toggle, "{pressed:?}");
    }
    let new_terminal = chord(
        &Key::Character("\u{f6}".into()),
        Physical::Code(Code::Semicolon),
        Modifiers::CTRL | Modifiers::SHIFT,
    );
    assert_eq!(new_terminal, "Ctrl+Shift+\u{d6}".parse::<Chord>().ok());
}

/// The terminal over the whole window (RC-040-05): Ctrl+A-umlaut on a German layout, Ctrl+' on
/// a US one - the key `Quote`, right of the toggle's key on both.
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
    if cfg!(not(target_os = "macos")) {
        assert_eq!(
            chord(&Key::Character("e".into()), ANY_KEY, Modifiers::LOGO),
            None
        );
    }
    assert_eq!(chord(&Key::Unidentified, ANY_KEY, Modifiers::empty()), None);
}

/// On a Mac Cmd is the keymap's Ctrl: Cmd+R is the chord Ctrl+R.
#[cfg(target_os = "macos")]
#[test]
fn cmd_is_the_keymaps_ctrl_on_macos() {
    let pressed = chord(&Key::Character("r".into()), ANY_KEY, Modifiers::LOGO);
    assert_eq!(
        pressed.map(|chord| chord.to_string()),
        Some("Ctrl+R".to_owned())
    );
}

/// `keys.macos_cmd_as_ctrl` (RC-100-49): on, Cmd+R is the chord Ctrl+R and Ctrl+R stays it;
/// off, a press with Cmd is no chord, as the Windows key is elsewhere, and Ctrl is still Ctrl.
#[test]
fn cmd_is_ctrl_only_while_the_setting_is_on() {
    use super::chord_with;

    let r = Key::Character("r".into());
    let pressed_as = |modifiers: Modifiers, cmd: bool| {
        chord_with(&r, ANY_KEY, modifiers, cmd).map(|c| c.to_string())
    };
    assert_eq!(pressed_as(Modifiers::LOGO, true).as_deref(), Some("Ctrl+R"));
    assert_eq!(pressed_as(Modifiers::CTRL, true).as_deref(), Some("Ctrl+R"));
    assert_eq!(
        pressed_as(Modifiers::LOGO | Modifiers::SHIFT, true).as_deref(),
        Some("Ctrl+Shift+R")
    );
    assert_eq!(pressed_as(Modifiers::LOGO, false), None);
    assert_eq!(
        pressed_as(Modifiers::CTRL, false).as_deref(),
        Some("Ctrl+R")
    );
}

#[test]
fn a_mac_label_shows_the_modifiers_as_symbols() {
    use super::mac_label;

    assert_eq!(mac_label("Ctrl+Shift+F", true), "⇧⌘F");
    assert_eq!(mac_label("Alt+F7", true), "⌥F7");
    assert_eq!(mac_label("Ctrl+ArrowUp", true), "⌘↑");
    assert_eq!(mac_label("F5", true), "F5");
    assert_eq!(mac_label("Ctrl+Plus", true), "⌘+");
}

/// With Cmd left to the system the keymap's Ctrl is the Mac's Control key, first in Apple's
/// order (RC-100-49).
#[test]
fn a_mac_label_shows_control_while_cmd_is_not_ctrl() {
    use super::mac_label;

    assert_eq!(mac_label("Ctrl+Shift+F", false), "⌃⇧F");
    assert_eq!(mac_label("Ctrl+Alt+F", false), "⌃⌥F");
    assert_eq!(mac_label("Alt+F7", false), "⌥F7");
    assert_eq!(mac_label("F5", false), "F5");
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

/// A dead key gives iced no character - the Spanish key right of n-tilde is the acute accent -:
/// with Ctrl it is a chord on its place and reaches the maximize; alone it starts a letter.
#[test]
fn a_dead_key_with_ctrl_is_a_chord_on_its_place() {
    let quote = Physical::Code(Code::Quote);
    let acute = chord(&Key::Unidentified, quote, Modifiers::CTRL).expect("a chord");
    assert_eq!(acute, "Ctrl+[Quote]".parse::<Chord>().expect("a chord"));
    assert_eq!(acute.physical, Some("Quote"));
    assert_eq!(
        Keymap::standard().lookup(&acute, PanelView::Terminal),
        Some(&Command::MaximizeTerminal { panel: None })
    );
    assert_eq!(chord(&Key::Unidentified, quote, Modifiers::empty()), None);
    assert_eq!(
        chord(
            &Key::Unidentified,
            Physical::Code(Code::KeyQ),
            Modifiers::CTRL
        ),
        None,
        "a key no chord names by place"
    );
    let altgr_dead = pressed(
        &Key::Unidentified,
        &Key::Unidentified,
        quote,
        Modifiers::ALT,
    );
    assert_eq!(altgr_dead, "Alt+[Quote]".parse::<Chord>().ok());
}

/// A press tells its place for the keys a chord can name by it, and only for those.
#[test]
fn a_press_tells_its_place_for_a_layout_key_only() {
    let semicolon = chord(
        &Key::Character("\u{f6}".into()),
        Physical::Code(Code::Semicolon),
        Modifiers::CTRL,
    );
    assert_eq!(
        semicolon.map(|chord| chord.physical),
        Some(Some("Semicolon"))
    );
    let letter = chord(
        &Key::Character("a".into()),
        Physical::Code(Code::KeyA),
        Modifiers::CTRL,
    );
    assert_eq!(letter.map(|chord| chord.physical), Some(None));
}

/// Off Windows no layout is asked: a key named by place reads as a US keyboard prints it.
#[cfg(not(any(windows, target_os = "macos")))]
#[test]
fn a_key_named_by_place_reads_as_on_a_us_keyboard_without_a_layout() {
    use super::label_text;

    assert_eq!(label_text("Ctrl+[Semicolon]"), "Ctrl+;");
    assert_eq!(label_text("Ctrl+Shift+[Quote]"), "Ctrl+Shift+'");
    assert_eq!(label_text("Ctrl+K"), "Ctrl+K");
    assert_eq!(label_text(""), "");
}

/// On a Mac the same keys read with the Mac's symbols.
#[cfg(target_os = "macos")]
#[test]
fn a_key_named_by_place_reads_with_mac_symbols() {
    use super::label_text;

    assert_eq!(label_text("Ctrl+[Semicolon]"), "⌘;");
    assert_eq!(label_text("Ctrl+Shift+[Quote]"), "⇧⌘'");
    assert_eq!(label_text("Ctrl+K"), "⌘K");
    assert_eq!(label_text(""), "");
}
