//! Drawing rCommander's own menu: a card of entries, each with its key, under the row it was
//! opened for, on a layer over the whole window so that a click beside it closes it.

use iced::{
    Alignment, Element, Length,
    mouse::Interaction,
    widget::{column, container, mouse_area, pin, row, stack},
};

use super::{
    ContextMessage, keys,
    menu::{self, Item, MENU_PADDING, MENU_WIDTH},
};
use crate::{
    app::{App, Message},
    theme::{self, Tokens, fit::line},
};

/// The window with rCommander's menu over it, once its place is known.
pub(in crate::app) fn with_menu<'a>(
    app: &'a App,
    base: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some(open) = &app.context_menu else {
        return base;
    };
    let Some(list) = open.list.filter(|_| !open.shell) else {
        return base;
    };
    let at = menu::place(open.items.len(), app.pane(open.panel), open.row, list);
    let entries = open
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| entry(item, index, index == open.cursor, tokens));
    let card = container(column(entries).padding(MENU_PADDING))
        .width(MENU_WIDTH)
        .style(move |_| theme::dialog(tokens));
    let close = Message::ContextMenu(ContextMessage::Close);
    let layer = mouse_area(pin(card).x(at.x).y(at.y))
        .on_press(close.clone())
        .on_right_press(close);
    stack![base, layer].into()
}

/// One entry: its label, its key (if any) on the right; filled while the keyboard or the pointer
/// is on it.
fn entry<'a>(
    item: &'a Item,
    index: usize,
    cursor: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let line = row![
        line(item.label).width(Length::Fill),
        line(item.key().and_then(keys::of_chord).unwrap_or_default()).color(tokens.text_secondary),
    ]
    .spacing(16)
    .padding([0, 10])
    .align_y(Alignment::Center);
    let face = container(line)
        .height(theme::ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .style(move |_| theme::row(tokens, cursor, true, false));
    mouse_area(face)
        .on_press(Message::ContextMenu(ContextMessage::Pick(
            item.choice.clone(),
        )))
        .on_enter(Message::ContextMenu(ContextMessage::Hover(index)))
        .interaction(Interaction::Pointer)
        .into()
}
