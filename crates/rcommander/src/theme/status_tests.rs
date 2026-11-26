use iced::Color;

use super::{MIN_BORDER_CONTRAST, Status, backgrounds, fit, readable};
use crate::theme::{
    colours::{MIN_CONTRAST, contrast},
    dark, hex, light,
    system::DEFAULT_ACCENT,
};

/// Fluent keeps WinUI's colours as they are: they read on its backgrounds without a step.
#[test]
fn fluent_keeps_winuis_status_colours() {
    assert_eq!(light::tokens(DEFAULT_ACCENT.light).status, Status::LIGHT);
    assert_eq!(dark::tokens(DEFAULT_ACCENT.dark).status, Status::DARK);
}

/// The dark look's status colours are its own: the light ones fell to 2.6:1 on its surface.
#[test]
fn the_dark_look_reads_its_status_colours() {
    let look = dark::tokens(DEFAULT_ACCENT.dark);
    for colour in [look.status.danger, look.status.warning, look.status.success] {
        for back in backgrounds(&look) {
            assert!(
                contrast(colour, back) >= MIN_CONTRAST,
                "{colour:?} on {back:?}"
            );
        }
    }
    assert!(contrast(Status::LIGHT.danger, look.surface) < MIN_CONTRAST);
}

/// A colour that falls short is darkened (light) or lightened (dark) just enough; where nothing
/// short of black or white reads, black or white it is.
#[test]
fn a_colour_moves_until_it_reads() {
    let tinted = hex(0xDCD5AC, 1.0);
    let moved = readable(Status::LIGHT.warning, false, &[tinted], MIN_CONTRAST);
    assert!(contrast(moved, tinted) >= MIN_CONTRAST);
    assert!(contrast(Status::LIGHT.warning, tinted) < MIN_CONTRAST);
    assert!(moved.r < Status::LIGHT.warning.r);
    let grey = hex(0x808080, 1.0);
    assert_eq!(
        readable(Status::DARK.danger, true, &[grey], 21.0),
        Color::WHITE
    );
    assert_eq!(
        readable(Status::LIGHT.danger, false, &[grey], 21.0),
        Color::BLACK
    );
}

/// A person's own theme with a surface of its own gets status colours and a field border that
/// read on it: a theme file names none of them.
#[test]
fn an_own_surface_gets_fitted_colours() {
    for (dark, surface) in [(false, 0xE6E7ED), (true, 0x4A4A4A)] {
        let mut look = if dark {
            dark::tokens(DEFAULT_ACCENT.dark)
        } else {
            light::tokens(DEFAULT_ACCENT.light)
        };
        look.surface = hex(surface, 1.0);
        look.list_fill = look.surface;
        fit(&mut look);
        let status = look.status;
        for back in backgrounds(&look) {
            for colour in [status.danger, status.warning, status.success] {
                assert!(
                    contrast(colour, back) >= MIN_CONTRAST,
                    "{colour:?} on {back:?}"
                );
            }
            assert!(contrast(status.field, back) >= MIN_BORDER_CONTRAST);
        }
    }
}
