use super::{BORDER, Measures, button_style, drawn};
use crate::{app::button_bar::BarSettings, theme};

fn measures(icon_size: u16, padding: u16) -> Measures {
    Measures::of(&BarSettings {
        icon_size,
        padding,
        ..BarSettings::default()
    })
}

/// RC-110-15: the symbol's size and the room around it make the button, in both bars; the
/// defaults keep 0.21's measures (a 24-pixel symbol, 5 pixels and the frame on each side).
#[test]
fn the_size_and_the_room_make_the_buttons_of_both_bars() {
    let standard = Measures::of(&BarSettings::default());
    assert_eq!(standard, measures(24, 5));
    assert_eq!(standard.button(24.0), 24.0 + 2.0 * (5.0 + BORDER));
    assert_eq!(standard.vertical_width(), 36.0);
    for size in [16, 24, 32, 40, 48] {
        let at = measures(size, 5);
        let side = f32::from(size) + 12.0;
        assert_eq!(at.button(f32::from(size)), side, "{size}");
        assert_eq!(at.vertical_height(12.0), side, "{size}");
    }
    // The vertical bar keeps room for words beside a small symbol.
    assert_eq!(measures(16, 5).vertical_width(), 36.0);
    assert_eq!(measures(48, 5).vertical_width(), 60.0);
    // No room: the frame alone; the most room: 12 on each side.
    assert_eq!(measures(24, 0).button(24.0), 26.0);
    assert_eq!(measures(24, 12).button(24.0), 50.0);
    assert_eq!(measures(32, 12).vertical_width(), 58.0);
}

/// A flat button draws nothing until the pointer or the keyboard is on it; a framed one always
/// draws its frame, as in 0.21.
#[test]
fn a_flat_button_shows_only_under_the_pointer_and_the_keyboard() {
    for dark in [false, true] {
        let tokens = theme::tokens(dark);
        assert!(!drawn(&button_style(tokens, true, false, false)));
        assert!(drawn(&button_style(tokens, true, false, true)), "hover");
        let focused = button_style(tokens, true, true, false);
        assert!(drawn(&focused), "focus");
        assert_eq!(focused.border.color, tokens.accent);
        assert!(drawn(&button_style(tokens, false, false, false)));
        assert_eq!(
            button_style(tokens, false, false, true).border.color,
            button_style(tokens, false, false, false).border.color,
            "a framed button looks as before under the pointer"
        );
    }
}
