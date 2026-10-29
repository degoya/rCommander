//! Drawing the queue window's context menu (`menu.rs`): a card of entries, each with its key, at
//! the top right of the job list, on a layer over the list so that a click beside it closes it;
//! an entry that does not apply to the selection is dimmed.

use iced::{
    Alignment, Element, Length,
    alignment::Horizontal,
    mouse::Interaction,
    widget::{column, container, mouse_area, row, stack},
};

use super::{
    QueueMsg, QueueWindow,
    menu::{MenuItem, MenuMsg},
};
use crate::{
    app::{Message, context_menu::keys},
    theme::{self, Tokens, fit::line},
};

const MENU_WIDTH: f32 = 300.0;

fn msg(message: MenuMsg) -> Message {
    Message::Queue(QueueMsg::Menu(message))
}

/// `list` with the open menu over it.
pub(super) fn with_menu<'a>(
    list: Element<'a, Message>,
    window: &'a QueueWindow,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(menu) = &window.menu else {
        return list;
    };
    let entries = menu
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| entry(item, index, index == menu.cursor, tokens));
    let card = container(column(entries).padding(4))
        .width(MENU_WIDTH)
        .style(move |_| theme::dialog(tokens));
    let close = msg(MenuMsg::Close);
    let layer = mouse_area(
        container(card)
            .padding(8)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(Horizontal::Right),
    )
    .on_press(close.clone())
    .on_right_press(close);
    stack![list, layer].into()
}

/// One entry: its label, its key (if any) on the right; filled while the keyboard or the pointer
/// is on it, dimmed where it does not apply.
fn entry<'a>(
    item: &'a MenuItem,
    index: usize,
    cursor: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let color = if item.enabled {
        tokens.text
    } else {
        tokens.text_secondary
    };
    let face = container(
        row![
            line(item.label.as_str()).color(color).width(Length::Fill),
            line(keys::of_text(item.key).unwrap_or_default()).color(tokens.text_secondary),
        ]
        .spacing(16)
        .padding([0, 10])
        .align_y(Alignment::Center),
    )
    .height(theme::ROW_HEIGHT)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .style(move |_| theme::row(tokens, cursor, true, false));
    mouse_area(face)
        .on_press(msg(MenuMsg::Pick(index)))
        .on_enter(msg(MenuMsg::Hover(index)))
        .interaction(if item.enabled {
            Interaction::Pointer
        } else {
            Interaction::Idle
        })
        .into()
}
