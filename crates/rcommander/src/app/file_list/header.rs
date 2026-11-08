//! The column header of the full view: each title's whole cell sorts (a click by the column,
//! again the other way round, as in Total Commander), filled while the pointer is on it; the
//! sorted column carries an arrow. Inside an archive the packed size, on a server the rights,
//! where switched on the owner, which sort by nothing. The cells have the rows' widths
//! (`columns.rs`) and font. The gap left of each fixed column's title is its grip (RC-010-20):
//! dragged, it sets the column's width - a column set's in the set (RC-100-90); double-clicked,
//! the default (`app/resize.rs`). A column set's titles are its own; a right click on the header
//! opens the menu of the column sets (Total Commander's `cm_SrcCustomViewMenu`).

use iced::{
    Element, Length, Padding,
    alignment::Horizontal,
    widget::{Space, button, column, container, mouse_area, row},
};
use rc_engine::{PanelId, SortKey};

use super::{
    columns::{Column, SCROLLBAR_WIDTH, SPACING, split_shown},
    divider,
    rows::sized,
    shown::Shown,
};
use crate::{
    app::{
        Message,
        panel::Pane,
        resize::{self, ResizeMsg},
    },
    i18n::t,
    theme::{self, Tokens, fonts},
};

/// The space above and below a title.
const TITLE_PAD: f32 = 3.0;

pub(super) fn column_header<'a>(
    pane: &'a Pane,
    side: PanelId,
    columns: &[Shown],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let sort = pane.state.sort;
    let font = fonts::list();
    let sizes = split_shown(pane.width(), columns, font.column_scale());
    // A title's height: its line (iced's default 1.3 of the size) and the space around it.
    let height = font.size * 1.3 + 2.0 * TITLE_PAD;
    // The pill's place and the gap after it, as in the rows.
    let mut titles: Vec<Element<'a, Message>> = vec![Space::new().width(3.0 + SPACING).into()];
    for (shown, size) in columns.iter().zip(sizes) {
        if let Some(target) = shown.grip {
            titles.push(resize::column_grip(target, SPACING, height));
        } else if shown.column != Column::Name {
            titles.push(Space::new().width(SPACING).height(height).into());
        }
        let name = shown.title.as_deref().unwrap_or(title(shown.column));
        let name = match shown.column.sort_key() {
            Some(key) if key == sort.key && sort.descending => {
                t!("column.sorted_down", column = name)
            }
            Some(key) if key == sort.key => t!("column.sorted_up", column = name),
            _ => name.to_owned(),
        };
        let label = container(sized(name, font.font, font.size).color(tokens.text_secondary))
            .width(Length::Fill)
            .align_x(if shown.right {
                Horizontal::Right
            } else {
                Horizontal::Left
            })
            .clip(true);
        let width = if shown.width.is_none() {
            Length::Fill
        } else {
            Length::Fixed(size)
        };
        titles.push(header_cell(
            label,
            width,
            shown.column.sort_key(),
            side,
            tokens,
        ));
    }
    let titles = mouse_area(
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
    )
    .on_right_press(Message::Resize(ResizeMsg::Menu(side)));
    column![titles, divider(tokens)].into()
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
        Column::Created => t!("column.field.created"),
        Column::Accessed => t!("column.field.accessed"),
        Column::Attributes => t!("column.field.attributes"),
        Column::User => t!("column.field.owner"),
        Column::Group => t!("column.field.group"),
        Column::Comment => t!("column.field.comment"),
        Column::Git => t!("column.field.git"),
    }
}
