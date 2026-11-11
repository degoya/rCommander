//! Drawing a bar's context menu (RC-110-15, `context.rs`): the file list's card
//! (`app/context_menu/view.rs`) - a check mark before flat buttons and the size in force, `>`
//! after Symbol size -, on a layer over the window that closes it at a click beside it.

use iced::{
    Alignment, Element, Length,
    mouse::Interaction,
    widget::{column, container, mouse_area, pin, row, stack, text},
};

use super::{
    BbMsg,
    context::{CtxMsg, Item},
    wrap,
};
use crate::{
    app::{
        App, Message,
        context_menu::{MENU_PADDING, MENU_WIDTH},
    },
    theme::{
        self, TEXT_SIZE, Tokens,
        ellipsis::middle_ellipsis,
        fit::line,
        icon::{self, Icon},
    },
};

/// The room of the check mark before a label, and of the arrow after it.
const MARK_WIDTH: f32 = 16.0;
/// The space a label has: the card less its padding, the marks and the spacing.
const LABEL_WIDTH: f32 = MENU_WIDTH - 2.0 * MENU_PADDING - 2.0 * 10.0 - 2.0 * MARK_WIDTH - 12.0;
/// The arrow of an entry that opens more.
const OPENS: &str = "\u{203a}";

fn menu_message(message: CtxMsg) -> Message {
    wrap(BbMsg::Ctx(message))
}

/// `base` with the open context menu over it, once its place is known.
pub fn with_context<'a>(
    app: &'a App,
    base: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some((menu, at)) = app
        .button_bar
        .context
        .as_ref()
        .and_then(|menu| menu.at.map(|at| (menu, at)))
    else {
        return base;
    };
    let entries = menu
        .items(&app.button_bar.settings)
        .into_iter()
        .enumerate()
        .map(|(index, item)| entry(item, index, index == menu.cursor, tokens));
    let card = container(column(entries).padding(MENU_PADDING))
        .width(MENU_WIDTH)
        .style(move |_| theme::dialog(tokens));
    let close = menu_message(CtxMsg::Close);
    // focus-ring: pointer only (the layer that closes the menu)
    let layer = mouse_area(pin(card).x(at.x).y(at.y))
        .on_press(close.clone())
        .on_right_press(close);
    stack![base, layer].into()
}

/// One entry: the check mark, the label, `>` for one that opens more; filled while the keyboard
/// or the pointer is on it.
fn entry<'a>(
    item: Item,
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
    let opens = if item.opens { OPENS } else { "" };
    let face = container(
        row![
            text(mark)
                .font(icon::FONT)
                .size(TEXT_SIZE)
                .width(MARK_WIDTH),
            line(label).width(Length::Fill),
            line(opens).color(tokens.text_secondary).width(MARK_WIDTH),
        ]
        .spacing(6)
        .padding([0, 10])
        .align_y(Alignment::Center),
    )
    .height(theme::ROW_HEIGHT)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .style(move |_| theme::row(tokens, cursor, true, false));
    // focus-ring: pointer only (an entry of the menu, worked by the menu's own keys)
    mouse_area(face)
        .on_enter(menu_message(CtxMsg::Hover(index)))
        .on_press(menu_message(CtxMsg::Pick(index)))
        .interaction(Interaction::Pointer)
        .into()
}
