//! The column header of the full view: each title's whole cell sorts (a click by the column,
//! again the other way round, as in Total Commander), filled while the pointer is on it; the
//! sorted column carries an arrow. Inside an archive the packed size, on a server the rights,
//! where switched on the owner, which sort by nothing. The cells have the rows' widths
//! (`columns.rs`) and font. The gap left of each fixed column's title is its grip (RC-010-20):
//! dragged, it sets the column's width; double-clicked, the default (`app/resize.rs`).

use iced::{
    Element, Length, Padding,
    alignment::Horizontal,
    widget::{Space, button, column, container, row},
};
use rc_engine::{PanelId, SortKey};

use super::{
    columns::{Column, ColumnWidths, SCROLLBAR_WIDTH, SPACING, split},
    divider,
    rows::sized,
    shown_columns,
};
use crate::{
    app::{Message, panel::Pane, resize},
    i18n::t,
    theme::{self, Tokens, fonts},
};

/// The space above and below a title.
const TITLE_PAD: f32 = 3.0;

pub(super) fn column_header<'a>(
    pane: &'a Pane,
    side: PanelId,
    widths: &ColumnWidths,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let sort = pane.state.sort;
    let font = fonts::list();
    let columns = shown_columns(pane, widths);
    let sizes = split(pane.width(), &columns, widths, font.column_scale());
    // A title's height: its line (iced's default 1.3 of the size) and the space around it.
    let height = font.size * 1.3 + 2.0 * TITLE_PAD;
    // The pill's place and the gap after it, as in the rows.
    let mut titles: Vec<Element<'a, Message>> = vec![Space::new().width(3.0 + SPACING).into()];
    for (shown, size) in columns.into_iter().zip(sizes) {
        if shown != Column::Name {
            titles.push(resize::column_grip(shown, SPACING, height));
        }
        let name = title(shown);
        let name = match shown.sort_key() {
            Some(key) if key == sort.key && sort.descending => {
                t!("column.sorted_down", column = name)
            }
            Some(key) if key == sort.key => t!("column.sorted_up", column = name),
            _ => name.to_owned(),
        };
        let label = container(sized(name, font.font, font.size).color(tokens.text_secondary))
            .width(Length::Fill)
            .align_x(if shown.right() {
                Horizontal::Right
            } else {
                Horizontal::Left
            })
            .clip(true);
        let width = if shown == Column::Name {
            Length::Fill
        } else {
            Length::Fixed(size)
        };
        titles.push(header_cell(label, width, shown.sort_key(), side, tokens));
    }
    column![
        container(
            row(titles)
                // The rows' own padding on the left; theirs plus the scrollbar on the right.
                .padding(Padding {
                    top: 2.0,
                    right: 10.0 + SCROLLBAR_WIDTH,
                    bottom: 2.0,
                    left: 10.0,
                }),
        )
        .width(Length::Fill),
        divider(tokens),
    ]
    .into()
}

/// A title's cell `width` wide: all of it sorts by `key`; without a key it is a plain cell.
fn header_cell<'a>(
    label: impl Into<Element<'a, Message>>,
    width: Length,
    key: Option<SortKey>,
    side: PanelId,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    button(label)
        .width(width)
        // No room at the sides: the title stands where the rows' values do.
        .padding([TITLE_PAD, 0.0])
        .style(move |_, status| theme::header_cell(tokens, status))
        .on_press_maybe(key.map(|key| Message::SortBy(side, key)))
        .into()
}

fn title(shown: Column) -> &'static str {
    match shown {
        Column::Name => t!("column.name"),
        Column::Ext => t!("column.ext"),
        Column::Size => t!("column.size"),
        Column::Packed => t!("column.packed"),
        Column::Date => t!("column.date"),
        Column::Rights => t!("column.rights"),
        Column::Owner => t!("column.owner"),
    }
}
