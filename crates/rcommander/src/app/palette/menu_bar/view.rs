//! Drawing the menu bar rCommander draws itself (RC-080-33, `drawn.rs`): the titles in a row at
//! the window's top, and the open menu on a layer over the whole window - the titles again, so
//! the pointer moves from one menu to the next, and the menu's card under its title, placed by
//! invisible copies of the titles before it - so that a click beside it closes it.

use iced::{
    Alignment, Color, Element, Length,
    mouse::Interaction,
    widget::{Space, column, container, mouse_area, row, stack},
};

use super::{MenuBarMsg, PaletteMsg};
use crate::{
    app::{App, Message, context_menu::keys},
    platform::menu_bar::{BarItem, BarMenu},
    theme::{self, Tokens, fit::line, layout},
};

/// The card's width, the context menu's.
const MENU_WIDTH: f32 = 280.0;
const MENU_PADDING: f32 = 4.0;
/// The window's padding round its content (`app/view.rs`), which the layer repeats.
const WINDOW_PADDING: f32 = 8.0;

fn message(message: MenuBarMsg) -> Message {
    Message::Palette(PaletteMsg::MenuBar(message))
}

/// The bar above the drive bar, while it is drawn and shown.
pub(in crate::app) fn bar<'a>(
    app: &'a App,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    if !app.drawn_bar_shown() {
        return None;
    }
    let bar = &app.dialog_windows.menu_bar;
    let enabled = bar.open.is_some() || !app.dialog_open();
    let open = bar.open.map(|open| open.menu);
    let titles = bar.menus.iter().enumerate().map(|(index, menu)| {
        let face = title(menu, open == Some(index), enabled, tokens);
        if enabled {
            mouse_area(face)
                .on_press(message(MenuBarMsg::Title(index)))
                .interaction(Interaction::Pointer)
                .into()
        } else {
            face
        }
    });
    Some(row(titles).into())
}

/// One title: its label with its access key, filled while its menu is open.
fn title<'a>(
    menu: &'a BarMenu,
    open: bool,
    enabled: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let color = (!enabled).then_some(tokens.text_secondary);
    container(layout::caption(&menu.title, color))
        .padding([0, 10])
        .height(theme::ROW_HEIGHT)
        .align_y(Alignment::Center)
        .style(move |_| theme::row(tokens, open, true, false))
        .into()
}

/// The window with the open menu over it.
pub(in crate::app) fn with_menu<'a>(
    app: &'a App,
    base: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let bar = &app.dialog_windows.menu_bar;
    let Some(open) = bar.open.filter(|_| app.drawn_bar_shown()) else {
        return base;
    };
    let Some(menu) = bar.menus.get(open.menu) else {
        return base;
    };
    let titles = bar.menus.iter().enumerate().map(|(index, menu)| {
        mouse_area(title(menu, index == open.menu, true, tokens))
            .on_press(message(MenuBarMsg::Title(index)))
            .on_enter(message(MenuBarMsg::HoverTitle(index)))
            .interaction(Interaction::Pointer)
            .into()
    });
    // The titles before the open one, invisible: the card starts where its title does.
    let before = bar.menus[..open.menu].iter().map(|menu| {
        container(layout::caption(&menu.title, Some(Color::TRANSPARENT)))
            .padding([0, 10])
            .into()
    });
    let entries = menu
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| entry(item, index, open.cursor == Some(index), tokens));
    let card = container(column(entries).padding(MENU_PADDING))
        .width(MENU_WIDTH)
        .style(move |_| theme::dialog(tokens));
    let layer = column![row(titles), row(before).push(card)];
    let close = message(MenuBarMsg::Close);
    let layer = mouse_area(
        container(layer)
            .padding(WINDOW_PADDING)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .on_press(close.clone())
    .on_right_press(close);
    stack![base, layer].into()
}

/// One entry: its label with its access key, its key on the right while the menus show keys; a
/// separator a line.
fn entry<'a>(
    item: &'a BarItem,
    index: usize,
    cursor: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let BarItem::Entry { id, text } = item else {
        let line = container(Space::new().height(1)).width(Length::Fill);
        return container(line.style(move |_| theme::divider(tokens)))
            .padding([4, 6])
            .into();
    };
    let (label, key) = text.split_once('\t').unwrap_or((text, ""));
    let key = if keys::shown() { key } else { "" };
    let face = row![
        container(layout::caption(label, None)).width(Length::Fill),
        line(key).color(tokens.text_secondary),
    ]
    .spacing(16)
    .padding([0, 10])
    .align_y(Alignment::Center);
    let face = container(face)
        .height(theme::ROW_HEIGHT)
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .style(move |_| theme::row(tokens, cursor, true, false));
    mouse_area(face)
        .on_press(message(MenuBarMsg::Chosen((*id).to_owned())))
        .on_enter(message(MenuBarMsg::HoverEntry(index)))
        .interaction(Interaction::Pointer)
        .into()
}
