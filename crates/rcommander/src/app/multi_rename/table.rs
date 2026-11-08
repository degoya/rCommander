//! The multi-rename dialog's table: old name, new name and conflict of the rows on screen.

use iced::{
    Alignment, Color, Element, Length,
    widget::{Column, column, container, mouse_area, row, sensor},
};
use rc_engine::{Conflict, SUBFOLDER_SEPARATOR};

use super::{MrMsg, MultiRenameDialog, controls::wrap};
use crate::{
    app::Message,
    i18n::{error_text, t},
    theme::{
        self, Tokens,
        fit::{self, line},
        focus,
    },
};

const CONFLICT_WIDTH: f32 = 300.0;

/// The rows on screen: old name, new name, conflict; a click puts the cursor on one, the wheel
/// scrolls, a click on the old name's header sorts by it.
pub(super) fn table<'a>(
    dialog: &'a MultiRenameDialog,
    tokens: &'static Tokens,
    danger: Color,
) -> Element<'a, Message> {
    let list = &dialog.list;
    // focus-ring: pointer only (the column header; its access key sorts by the old name too)
    let sort = mouse_area(
        line(t!("multi_rename.column_old"))
            .color(tokens.text_secondary)
            .width(Length::Fill),
    )
    .on_press(wrap(MrMsg::SortByName));
    let header = row![
        sort,
        line(t!("multi_rename.column_new"))
            .color(tokens.text_secondary)
            .width(Length::Fill),
        line(t!("multi_rename.column_conflict"))
            .color(tokens.text_secondary)
            .width(CONFLICT_WIDTH),
    ]
    .spacing(8)
    .padding([0, 6]);
    let rows = list.visible().filter_map(|index| {
        let (old, item) = list.row(index)?;
        let new = item.map_or_else(String::new, |item| shown(&item.new));
        let conflict = item.and_then(|item| item.conflict);
        let reason = conflict.map_or_else(String::new, |conflict| conflict_text(conflict, dialog));
        let color = if conflict.is_some() {
            danger
        } else {
            tokens.text
        };
        // Names are shortened in the middle, the reason cut at its end; each whole in a tooltip when
        // it does not fit.
        let reason_cell = container(line(reason.clone()).color(danger))
            .width(CONFLICT_WIDTH)
            .clip(true);
        let reason_cell = if fit::overflows(&reason, CONFLICT_WIDTH) {
            fit::tip(reason_cell, reason, tokens)
        } else {
            reason_cell.into()
        };
        let cells = row![
            fit::path_cell(old, 1, false, tokens),
            container(fit::path_line(new, color, tokens)).width(Length::Fill),
            reason_cell,
        ]
        .spacing(8)
        .padding([0, 6])
        .align_y(Alignment::Center);
        let cursor = index == list.cursor();
        let framed = container(cells)
            .height(theme::ROW_HEIGHT)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| {
                theme::striped_row(tokens, cursor, true, false, theme::alternate(index))
            });
        Some(
            // focus-ring: pointer only (a row of the rows choice stop)
            mouse_area(framed)
                .on_press(wrap(MrMsg::Click(index)))
                .into(),
        )
    });
    let body = container(Column::with_children(rows))
        .height(Length::Fill)
        .width(Length::Fill)
        .clip(true);
    let body = mouse_area(body)
        .on_scroll(|delta| wrap(MrMsg::Scroll(theme::wheel::rows(delta, theme::ROW_HEIGHT))));
    // The table reports its height, so as many rows are drawn as fit.
    let body = sensor(body)
        .on_show(|size| wrap(MrMsg::Viewport(theme::rows_for(size.height))))
        .on_resize(|size| wrap(MrMsg::Viewport(theme::rows_for(size.height))));
    // The rows are one stop of the ring; the dialog moves their cursor with the arrows.
    let body = focus::choice(body, "multi_rename.rows", None, None);
    column![header, body].spacing(4).height(Length::Fill).into()
}

/// A new name as the platform spells a path: `sub\name` on Windows.
fn shown(name: &str) -> String {
    name.replace(SUBFOLDER_SEPARATOR, std::path::MAIN_SEPARATOR_STR)
}

/// Why a row cannot be renamed; a duplicate names the other entry.
fn conflict_text(conflict: Conflict, dialog: &MultiRenameDialog) -> String {
    let reason = error_text(conflict.code());
    match conflict {
        Conflict::Duplicate { other } => match dialog.list.row(other) {
            Some((name, _)) => t!("multi_rename.duplicate_of", reason = reason, name = name),
            None => reason,
        },
        _ => reason,
    }
}
