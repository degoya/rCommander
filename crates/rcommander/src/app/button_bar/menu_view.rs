//! Drawing the bars' menus over the window (RC-100-87): a sub-bar's or `»`'s items with their
//! symbols and words under the button they opened from (`menu.rs`), and the bar's context menu
//! (`context_view.rs`, RC-110-15). A click beside a menu closes it. Split from `view.rs`.

use iced::{
    Alignment, Element, Length,
    mouse::Interaction,
    widget::{Column, Row, Space, column, container, mouse_area, pin, rule, stack},
};

use super::{BbMsg, MenuKind, MenuMsg, OpenMenu, context_view, view, wrap};
use crate::{
    app::{
        App, Message,
        drive_bar::{button_style, card_style},
    },
    theme::{self, Tokens, ellipsis},
};

/// The widest a menu gets.
const MENU_WIDTH: f32 = 280.0;

/// `base` with the open menus over it: a sub-bar's once its button is measured, the context
/// menu once placed.
pub fn with_menu<'a>(
    app: &'a App,
    base: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let base = context_view::with_context(app, base, tokens);
    let Some(menu) = app.button_bar.menu.as_ref() else {
        return base;
    };
    let Some(under) = menu.under else {
        return base;
    };
    let card = container(menu_rows(app, menu, tokens))
        .padding(4)
        .width(MENU_WIDTH)
        .style(move |_| card_style(tokens));
    let x = under.x;
    let y = under.y + under.height + 2.0;
    // focus-ring: pointer only (the layer beside the menu that closes it)
    let layer = mouse_area(pin(card).x(x).y(y).width(Length::Fill).height(Length::Fill))
        .on_press(wrap(BbMsg::Menu(MenuMsg::Close)))
        .on_right_press(wrap(BbMsg::Menu(MenuMsg::Close)));
    stack![base, layer].into()
}

/// The rows of `menu`: a bar's items with their faces and words.
fn menu_rows<'a>(app: &'a App, menu: &OpenMenu, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut rows = Column::new().spacing(1);
    let MenuKind::Items { bar, from } = menu.kind;
    let Some(bar) = app.button_bar.bar(bar) else {
        return rows.into();
    };
    for (index, item) in bar.spec.items.iter().enumerate().skip(from) {
        let Some(button) = item.button() else {
            rows = rows.push(rule::horizontal(1));
            continue;
        };
        let words = view::label_of(button, app);
        let cut =
            ellipsis::middle_ellipsis(&words, MENU_WIDTH - 60.0, theme::TEXT_SIZE).into_owned();
        let line = Row::new()
            .push(
                container(view::symbol(button, app).unwrap_or_else(|| Space::new().into()))
                    .width(32),
            )
            .push(view::small(cut))
            .spacing(6)
            .align_y(Alignment::Center);
        rows = rows.push(menu_row(line.into(), index, menu.cursor == index, tokens));
    }
    column![rows].into()
}

/// One row of a menu, filled at the cursor, `row` picked by a click.
fn menu_row<'a>(
    content: Element<'a, Message>,
    row: usize,
    current: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let boxed = container(content)
        .padding([3, 8])
        .width(Length::Fill)
        .style(move |_| {
            if current {
                button_style(tokens, true)
            } else {
                container::Style::default()
            }
        });
    // focus-ring: pointer only (a row of the menu, which the arrows run)
    mouse_area(boxed)
        .on_press(wrap(BbMsg::Menu(MenuMsg::Pick(row))))
        .interaction(Interaction::Pointer)
        .into()
}
