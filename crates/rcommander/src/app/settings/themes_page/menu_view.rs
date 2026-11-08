//! Drawing the context menu of a theme's row (RC-110-09, `menu.rs`): the file list's card
//! (`app/context_menu/view.rs`) over the Themes tab's scrolled area - a check mark before an
//! environment the theme is used for already, an entry that cannot run greyed out -, on a layer
//! that closes it at a click or a scroll beside it.

use iced::{
    Alignment, Element, Length,
    mouse::Interaction,
    widget::{column, container, mouse_area, pin, row, stack, text},
};

use super::{
    ThemesMsg, ThemesPage,
    menu::{Item, MenuMsg},
    page_message,
};
use crate::{
    app::{
        Message,
        context_menu::{MENU_PADDING, MENU_WIDTH},
    },
    theme::{
        self, TEXT_SIZE, Tokens,
        ellipsis::middle_ellipsis,
        fit::line,
        icon::{self, Icon},
    },
};

/// The room of the check mark before a label.
const MARK_WIDTH: f32 = 16.0;
/// The space a label has: the card less its padding, the mark and the spacing.
const LABEL_WIDTH: f32 = MENU_WIDTH - 2.0 * MENU_PADDING - 2.0 * 10.0 - MARK_WIDTH - 6.0;

fn menu_message(message: MenuMsg) -> Message {
    page_message(ThemesMsg::Menu(message))
}

/// `area`, the tab's scrolled area, with the open menu over it once its place is known.
pub(in crate::app::settings) fn with_menu<'a>(
    page: &'a ThemesPage,
    area: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some((menu, at)) = page
        .menu
        .as_ref()
        .filter(|_| page.editor.is_none())
        .and_then(|menu| menu.at.map(|at| (menu, at)))
    else {
        return area;
    };
    let entries = menu
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| entry(item, index, index == menu.cursor, tokens));
    let card = container(column(entries).padding(MENU_PADDING))
        .width(MENU_WIDTH)
        .style(move |_| theme::dialog(tokens));
    let close = menu_message(MenuMsg::Close);
    let scrolled = close.clone();
    // focus-ring: pointer only (the layer that closes the menu)
    let layer = mouse_area(pin(card).x(at.x).y(at.y))
        .on_press(close.clone())
        .on_right_press(close)
        .on_scroll(move |_| scrolled.clone());
    stack![area, layer].into()
}

/// One entry: the check mark where the theme is used already, the label; filled while the
/// keyboard or the pointer is on it, dimmed when it cannot run.
fn entry<'a>(
    item: &'a Item,
    index: usize,
    cursor: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mark = if item.checked {
        Icon::Confirm.glyph().to_string()
    } else {
        String::new()
    };
    let label = middle_ellipsis(&item.label, LABEL_WIDTH, TEXT_SIZE).into_owned();
    let mut name = line(label).width(Length::Fill);
    if !item.enabled {
        name = name.color(tokens.text_secondary);
    }
    let face = container(
        row![
            text(mark)
                .font(icon::FONT)
                .size(TEXT_SIZE)
                .width(MARK_WIDTH),
            name,
        ]
        .spacing(6)
        .padding([0, 10])
        .align_y(Alignment::Center),
    )
    .height(theme::ROW_HEIGHT)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .style(move |_| theme::row(tokens, cursor && item.enabled, true, false));
    // focus-ring: pointer only (an entry of the menu, worked by the menu's own keys)
    let mut area = mouse_area(face)
        .on_enter(menu_message(MenuMsg::Hover(index)))
        .interaction(Interaction::Pointer);
    if item.enabled {
        area = area.on_press(menu_message(MenuMsg::Pick(index)));
    }
    area.into()
}
