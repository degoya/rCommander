use iced::{Background, widget::button::Status};

use super::{alternate_when, drive_line, drive_line_rule, header_cell, striped_row, tab};
use crate::theme::tokens;

fn fill(background: Option<Background>) -> Option<iced::Color> {
    match background {
        Some(Background::Color(color)) => Some(color),
        _ => None,
    }
}

#[test]
fn the_stripe_takes_every_odd_row_while_it_is_on() {
    assert!(!alternate_when(true, 0), "`..` is row 0, not striped");
    assert!(alternate_when(true, 1));
    assert!(!alternate_when(true, 2));
    assert!(alternate_when(true, 3));
    for index in 0..4 {
        assert!(
            !alternate_when(false, index),
            "off: no stripe on row {index}"
        );
    }
}

#[test]
fn cursor_and_selection_win_over_the_stripe() {
    for dark in [false, true] {
        let tokens = tokens(dark);
        let even = striped_row(tokens, false, true, false, false);
        let odd = striped_row(tokens, false, true, false, true);
        assert_eq!(fill(even.background), None);
        assert_eq!(fill(odd.background), Some(tokens.stripe_fill));
        let cursor = striped_row(tokens, true, true, false, true);
        assert_eq!(fill(cursor.background), Some(tokens.cursor_fill));
        let inactive = striped_row(tokens, true, false, false, true);
        assert_eq!(fill(inactive.background), Some(tokens.cursor_fill_inactive));
        let selected = striped_row(tokens, false, true, true, true);
        assert_eq!(fill(selected.background), Some(tokens.selected_fill));
    }
}

#[test]
fn the_active_tab_has_the_list_s_colour_and_the_others_fill_on_hover() {
    let tokens = tokens(true);
    assert_eq!(
        fill(tab(tokens, true, Status::Active).background),
        Some(tokens.surface)
    );
    assert_eq!(fill(tab(tokens, false, Status::Active).background), None);
    assert_eq!(
        fill(tab(tokens, false, Status::Hovered).background),
        Some(tokens.cursor_fill_inactive)
    );
    assert_eq!(
        tab(tokens, false, Status::Active).text_color,
        tokens.text_secondary
    );
    assert_eq!(fill(header_cell(tokens, Status::Active).background), None);
    assert!(header_cell(tokens, Status::Hovered).background.is_some());
}

#[test]
fn the_drive_line_stands_out_and_the_active_panel_s_in_the_accent() {
    for dark in [false, true] {
        let tokens = tokens(dark);
        assert_eq!(
            fill(drive_line(tokens, false).background),
            Some(tokens.header)
        );
        assert_eq!(
            fill(drive_line(tokens, true).background),
            Some(tokens.cursor_fill)
        );
        assert_eq!(
            fill(drive_line_rule(tokens, false).background),
            Some(tokens.stroke)
        );
        assert_eq!(
            fill(drive_line_rule(tokens, true).background),
            Some(tokens.accent)
        );
    }
}
