use alacritty_terminal::{
    index::{Column, Line, Point as GridPoint, Side},
    selection::{Selection, SelectionType},
    term::{TermMode, test::mock_term},
};
use iced::{
    Point, Size,
    keyboard::{Key, Modifiers, key::Named},
    mouse::ScrollDelta,
};
use rc_term::TermSize;

use super::*;
use crate::bindings::BindingsLayout;

fn typed(key: Key, modifiers: Modifiers, text: Option<&str>) -> KeyInput {
    key_input(
        &BindingsLayout::default(),
        &key,
        modifiers,
        text,
        TermMode::empty(),
    )
}

fn bytes(text: &str) -> KeyInput {
    KeyInput::Write(text.as_bytes().to_vec())
}

/// The keys the file list owns reach the shell in a terminal (RC-040-03): Tab, F5, Ctrl+C and
/// the arrows.
#[test]
fn the_panel_keys_go_to_the_shell() {
    let none = Modifiers::empty();
    assert_eq!(typed(Key::Named(Named::Tab), none, Some("\t")), bytes("\t"));
    assert_eq!(typed(Key::Named(Named::F5), none, None), bytes("\x1b[15~"));
    assert_eq!(
        typed(Key::Character("c".into()), Modifiers::CTRL, None),
        bytes("\x03")
    );
    for (named, sequence) in [
        (Named::ArrowUp, "\x1b[A"),
        (Named::ArrowDown, "\x1b[B"),
        (Named::ArrowRight, "\x1b[C"),
        (Named::ArrowLeft, "\x1b[D"),
    ] {
        assert_eq!(typed(Key::Named(named), none, None), bytes(sequence));
    }
    assert_eq!(
        typed(Key::Named(Named::Tab), Modifiers::CTRL, None),
        KeyInput::None,
        "Ctrl+Tab is no panel switch and sends nothing either"
    );
}

#[test]
fn unbound_keys_type_their_text_and_alt_prefixes_escape() {
    assert_eq!(
        typed(Key::Character("a".into()), Modifiers::empty(), Some("a")),
        bytes("a")
    );
    assert_eq!(
        typed(Key::Character("A".into()), Modifiers::SHIFT, Some("A")),
        bytes("A")
    );
    assert_eq!(
        typed(Key::Character("x".into()), Modifiers::ALT, Some("x")),
        bytes("\x1bx")
    );
    assert_eq!(
        typed(
            Key::Character("q".into()),
            Modifiers::CTRL | Modifiers::ALT,
            Some("@")
        ),
        bytes("@"),
        "AltGr types its character"
    );
    assert_eq!(
        typed(Key::Named(Named::Space), Modifiers::SHIFT, Some(" ")),
        bytes(" ")
    );
    assert_eq!(
        typed(Key::Unidentified, Modifiers::empty(), None),
        KeyInput::None
    );
}

#[test]
fn a_paste_with_a_line_break_asks_and_is_bracketed_when_wanted() {
    assert!(!acts_beyond_typing("ls -la"));
    assert!(acts_beyond_typing("ls\nrm -rf x"));
    assert!(acts_beyond_typing("one\r"));
    assert_eq!(paste_bytes("a\nb\r\nc", TermMode::empty()), b"a\rb\rc");
    assert_eq!(
        paste_bytes("a\x1b[201~b", TermMode::BRACKETED_PASTE),
        b"\x1b[200~a[201~b\x1b[201~",
        "no escape inside the bracket"
    );
}

/// RC-040-05: a terminal laid over both panels fits its grid to the wider area - the columns of
/// both panels and of the gap between them, the same rows - which the view's `fit` gives the
/// session (its PTY) once the layout hands it the new size.
#[test]
fn the_grid_follows_the_width_of_the_area() {
    let cell = Size::new(8.0, 16.0);
    let panel = Size::new(800.0, 400.0);
    // Both panels and the 8-pixel gap between them.
    let both = Size::new(2.0 * panel.width + 8.0, panel.height);
    assert_eq!(grid_size(panel, cell), TermSize::new(25, 100).ok());
    assert_eq!(grid_size(both, cell), TermSize::new(25, 201).ok());
}

#[test]
fn pixels_map_to_cells() {
    let cell = Size::new(8.0, 16.0);
    assert_eq!(
        grid_size(Size::new(800.0, 400.0), cell),
        Some(TermSize {
            rows: 25,
            cols: 100
        })
    );
    assert_eq!(grid_size(Size::new(7.0, 400.0), cell), None);
    assert_eq!(grid_size(Size::new(0.0, 0.0), cell), None);
    assert_eq!(
        grid_point(Point::new(17.0, 33.0), cell, 100, 25, 0),
        GridPoint::new(Line(2), Column(2))
    );
    assert_eq!(
        grid_point(Point::new(5_000.0, -3.0), cell, 100, 25, 0),
        GridPoint::new(Line(0), Column(99)),
        "clamped to the grid"
    );
    assert_eq!(
        grid_point(Point::new(0.0, 0.0), cell, 100, 25, 10),
        GridPoint::new(Line(-10), Column(0)),
        "scrolled into the history"
    );
    assert_eq!(side(3.0, cell), Side::Left);
    assert_eq!(side(13.0, cell), Side::Right);
}

#[test]
fn mouse_reports_in_sgr_and_x10() {
    let point = GridPoint::new(Line(4), Column(9));
    assert_eq!(
        mouse_report(
            LEFT_BUTTON,
            Modifiers::empty(),
            point,
            true,
            TermMode::SGR_MOUSE
        ),
        Some(b"\x1b[<0;10;5M".to_vec())
    );
    assert_eq!(
        mouse_report(
            LEFT_BUTTON,
            Modifiers::SHIFT,
            point,
            false,
            TermMode::SGR_MOUSE
        ),
        Some(b"\x1b[<4;10;5m".to_vec())
    );
    assert_eq!(
        mouse_report(
            LEFT_BUTTON,
            Modifiers::empty(),
            point,
            true,
            TermMode::empty()
        ),
        Some(vec![0x1b, b'[', b'M', 32, 32 + 10, 32 + 5])
    );
    assert_eq!(
        mouse_report(
            LEFT_MOVE,
            Modifiers::empty(),
            point,
            false,
            TermMode::empty()
        ),
        Some(vec![0x1b, b'[', b'M', 32 + 3, 32 + 10, 32 + 5]),
        "X10 reports every release as button 3"
    );
    let far = GridPoint::new(Line(0), Column(300));
    assert_eq!(
        mouse_report(
            LEFT_BUTTON,
            Modifiers::empty(),
            far,
            true,
            TermMode::empty()
        ),
        None
    );
    assert!(
        mouse_report(
            LEFT_BUTTON,
            Modifiers::empty(),
            far,
            true,
            TermMode::UTF8_MOUSE
        )
        .is_some()
    );
}

#[test]
fn the_wheel_scrolls_three_lines_a_notch_and_keeps_pixel_rests() {
    let mut rest = 0.0;
    assert_eq!(
        wheel_lines(ScrollDelta::Lines { x: 0.0, y: 1.0 }, &mut rest, 16.0),
        3
    );
    assert_eq!(
        wheel_lines(ScrollDelta::Lines { x: 0.0, y: -2.0 }, &mut rest, 16.0),
        -6
    );
    assert_eq!(
        wheel_lines(ScrollDelta::Pixels { x: 0.0, y: 40.0 }, &mut rest, 16.0),
        2
    );
    assert_eq!(rest, 8.0);
    assert_eq!(
        wheel_lines(ScrollDelta::Pixels { x: 0.0, y: 8.0 }, &mut rest, 16.0),
        1
    );
    assert_eq!(alternate_scroll(2), b"\x1bOA\x1bOA");
    assert_eq!(alternate_scroll(-1), b"\x1bOB");
}

#[test]
fn only_a_plain_ctrl_c_is_the_copy_chord() {
    assert!(is_ctrl_c(&Key::Character("c".into()), Modifiers::CTRL));
    assert!(is_ctrl_c(&Key::Character("C".into()), Modifiers::CTRL));
    assert!(!is_ctrl_c(
        &Key::Character("c".into()),
        Modifiers::CTRL | Modifiers::SHIFT
    ));
    assert!(!is_ctrl_c(&Key::Character("c".into()), Modifiers::empty()));
    assert!(!is_ctrl_c(&Key::Character("v".into()), Modifiers::CTRL));
}

#[test]
fn shift_keeps_the_mouse_for_selecting_only_when_allowed() {
    let reporting = TermMode::MOUSE_REPORT_CLICK | TermMode::MOUSE_DRAG;
    assert!(mouse_to_program(reporting, Modifiers::empty(), true));
    assert!(!mouse_to_program(reporting, Modifiers::SHIFT, true));
    assert!(mouse_to_program(reporting, Modifiers::SHIFT, false));
    assert!(!mouse_to_program(
        TermMode::empty(),
        Modifiers::empty(),
        true
    ));
}

#[test]
fn only_a_plain_ctrl_v_is_the_paste_chord() {
    assert!(is_ctrl_v(&Key::Character("v".into()), Modifiers::CTRL));
    assert!(is_ctrl_v(&Key::Character("V".into()), Modifiers::CTRL));
    assert!(!is_ctrl_v(
        &Key::Character("v".into()),
        Modifiers::CTRL | Modifiers::SHIFT
    ));
    assert!(!is_ctrl_v(&Key::Character("v".into()), Modifiers::ALT));
    assert!(!is_ctrl_v(&Key::Character("c".into()), Modifiers::CTRL));
}

#[test]
fn a_selection_is_taken_and_cleared_only_when_asked() {
    let mut term = mock_term("hello\r\nworld");
    assert_eq!(take_selection(&mut term, true), None);

    let start = GridPoint::new(Line(0), Column(0));
    let mut selection = Selection::new(SelectionType::Simple, start, Side::Left);
    selection.update(GridPoint::new(Line(0), Column(4)), Side::Right);
    term.selection = Some(selection);
    assert_eq!(take_selection(&mut term, false).as_deref(), Some("hello"));
    assert!(term.selection.is_some());
    assert_eq!(take_selection(&mut term, true).as_deref(), Some("hello"));
    assert!(term.selection.is_none());

    // A click without a drag leaves an empty selection: nothing to copy.
    term.selection = Some(Selection::new(SelectionType::Simple, start, Side::Left));
    assert_eq!(take_selection(&mut term, true), None);
}

#[test]
fn a_paste_with_a_control_character_but_a_tab_asks() {
    for text in [
        "make\x03",
        "keep\x15rm -rf x",
        "word\x17",
        "ab\x08c",
        "\x1b[201~",
        "del\x7f",
        "c1\u{9b}",
    ] {
        assert!(acts_beyond_typing(text), "{text:?}");
    }
    assert!(!acts_beyond_typing("a\tb"), "a tab is typed as it is");
    assert!(!acts_beyond_typing("caf\u{e9} \u{65e5}\u{672c}"));
}
