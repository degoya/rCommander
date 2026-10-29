use alacritty_terminal::term::TermMode;
use iced::keyboard::{Modifiers, key::Named};

use super::{Binding, BindingAction, BindingsLayout, InputKind};

fn action(input: InputKind, modifiers: Modifiers, mode: TermMode) -> BindingAction {
    BindingsLayout::default().action(&input, modifiers, mode)
}

fn key(named: Named) -> InputKind {
    InputKind::KeyCode(named)
}

fn letter(text: &str) -> InputKind {
    InputKind::Char(text.to_owned())
}

#[test]
fn every_binding_finds_its_own_action() {
    let layout = BindingsLayout::default();
    for (binding, expected) in &layout.layout {
        // The first row of a key wins; a later row for the same key, modifiers and modes would
        // be shadowed, which `add_bindings` never lets happen.
        let found = layout.action(&binding.target, binding.modifiers, binding.mode_include);
        assert_eq!(&found, expected, "{binding:?}");
    }
}

#[test]
fn the_keys_a_shell_needs_send_their_sequences() {
    let none = Modifiers::empty();
    let plain = TermMode::empty();
    assert_eq!(
        action(key(Named::Tab), none, plain),
        BindingAction::Char('\t')
    );
    assert_eq!(
        action(key(Named::F5), none, plain),
        BindingAction::Esc("\x1b[15~".into())
    );
    assert_eq!(
        action(letter("c"), Modifiers::CTRL, plain),
        BindingAction::Char('\x03')
    );
    assert_eq!(
        action(letter("u"), Modifiers::CTRL, plain),
        BindingAction::Char('\x15'),
        "iced_term sent Q"
    );
    assert_eq!(
        action(key(Named::ArrowUp), none, plain),
        BindingAction::Esc("\x1b[A".into())
    );
    assert_eq!(
        action(key(Named::ArrowUp), none, TermMode::APP_CURSOR),
        BindingAction::Esc("\x1bOA".into())
    );
}

#[cfg(not(target_os = "macos"))]
#[test]
fn ctrl_shift_c_and_v_copy_and_paste() {
    let chord = Modifiers::CTRL | Modifiers::SHIFT;
    assert_eq!(
        action(letter("c"), chord, TermMode::empty()),
        BindingAction::Copy
    );
    assert_eq!(
        action(letter("v"), chord, TermMode::empty()),
        BindingAction::Paste
    );
    assert_eq!(
        action(letter("x"), chord, TermMode::empty()),
        BindingAction::Char('\x18')
    );
}

#[test]
fn shift_page_keys_scroll_the_history_but_reach_a_full_screen_program() {
    let shift = Modifiers::SHIFT;
    assert_eq!(
        action(key(Named::PageUp), shift, TermMode::empty()),
        BindingAction::ScrollPageUp
    );
    assert_eq!(
        action(key(Named::PageDown), shift, TermMode::empty()),
        BindingAction::ScrollPageDown
    );
    assert_eq!(
        action(key(Named::PageUp), shift, TermMode::ALT_SCREEN),
        BindingAction::Esc("\x1b[5;2~".into())
    );
}

#[test]
fn an_added_binding_replaces_the_one_it_matches() {
    let mut layout = BindingsLayout::default();
    let before = layout.layout.len();
    let binding = Binding {
        target: key(Named::F12),
        modifiers: Modifiers::empty(),
        mode_include: TermMode::empty(),
        mode_exclude: TermMode::empty(),
    };
    layout.add_bindings(vec![(binding.clone(), BindingAction::Ignore)]);
    assert_eq!(layout.layout.len(), before);
    assert_eq!(
        layout.action(&binding.target, Modifiers::empty(), TermMode::empty()),
        BindingAction::Ignore
    );
}
