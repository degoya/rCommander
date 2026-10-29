//! The find-files dialog's hits: path, size and date under their header, as many as fit.

use iced::{
    Alignment, Element, Length,
    alignment::Horizontal,
    widget::{Column, column, container, mouse_area, row, sensor},
};

use super::wrap;
use crate::{
    app::{
        Message,
        search::{SearchDialog, SearchMsg},
    },
    format,
    i18n::t,
    theme::{
        self, Tokens,
        fit::{self, line},
        focus,
    },
};

const SIZE_WIDTH: f32 = 96.0;
const DATE_WIDTH: f32 = 128.0;

/// The hits on screen: path, size, date; a click puts the cursor on one, a double-click goes
/// to it, the wheel scrolls.
pub(super) fn hit_list<'a>(
    dialog: &'a SearchDialog,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let results = &dialog.results;
    let header = row![
        line(t!("search.column_path"))
            .color(tokens.text_secondary)
            .width(Length::Fill),
        line(t!("column.size"))
            .color(tokens.text_secondary)
            .width(SIZE_WIDTH)
            .align_x(Horizontal::Right),
        line(t!("column.date"))
            .color(tokens.text_secondary)
            .width(DATE_WIDTH),
    ]
    .spacing(8)
    .padding([0, 6]);
    let rows = results.visible().map(|index| {
        let hit = &results.hits()[index];
        let size = if hit.entry.is_dir_like() {
            t!("panel.dir_marker").to_owned()
        } else {
            format::size(hit.entry.size)
        };
        let cells = row![
            fit::path_cell(hit.path.path.display().to_string(), 1, false, tokens),
            line(size).width(SIZE_WIDTH).align_x(Horizontal::Right),
            line(format::date(hit.entry.modified)).width(DATE_WIDTH),
        ]
        .spacing(8)
        .padding([0, 6])
        .align_y(Alignment::Center);
        let cursor = index == results.cursor();
        let framed = container(cells)
            .height(theme::ROW_HEIGHT)
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| {
                theme::striped_row(tokens, cursor, true, false, theme::alternate(index))
            });
        mouse_area(framed)
            .on_press(wrap(SearchMsg::Click(index)))
            .on_double_click(wrap(SearchMsg::Open(index)))
            .into()
    });
    let list = container(Column::with_children(rows))
        .height(Length::Fill)
        .width(Length::Fill)
        .clip(true);
    let list = mouse_area(list).on_scroll(|delta| {
        wrap(SearchMsg::Scroll(theme::wheel::rows(
            delta,
            theme::ROW_HEIGHT,
        )))
    });
    // The list reports its height, so as many hits are drawn as fit.
    let list = sensor(list)
        .on_show(|size| wrap(SearchMsg::Viewport(theme::rows_for(size.height))))
        .on_resize(|size| wrap(SearchMsg::Viewport(theme::rows_for(size.height))));
    // The hits are one stop of the ring; the dialog moves their cursor with the arrows.
    let list = focus::choice(list, "search.hits", None, None);
    column![header, list].spacing(4).height(Length::Fill).into()
}
