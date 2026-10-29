//! Drawing the command palette: a card at the window's top over the dimmed window, its field and
//! a page of rows around the chosen one, each with its name and its keys.

use iced::{
    Element, Length,
    widget::{Space, column, container, mouse_area, opaque, row, text_input},
};

use super::{FIELD_ID, PAGE, Palette, PaletteMsg};
use crate::{
    app::Message,
    i18n::t,
    theme::{self, Tokens, fit::line},
};

const CARD_WIDTH: f32 = 560.0;

/// The palette over the window.
pub fn overlay<'a>(palette: &'a Palette, tokens: &'static Tokens) -> Element<'a, Message> {
    let field = text_input(t!("palette.placeholder"), &palette.query)
        .id(FIELD_ID)
        .on_input(|query| Message::Palette(PaletteMsg::Query(query)))
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([6, 8]);
    let mut body = column![field].spacing(6);
    if palette.shown.is_empty() {
        body = body.push(line(t!("palette.empty")).color(tokens.text_secondary));
    }
    // The page that holds the chosen row.
    let first = palette.selected.saturating_sub(PAGE - 1);
    for (at, index) in palette.shown.iter().enumerate().skip(first).take(PAGE) {
        let Some(entry) = palette.entry(*index) else {
            continue;
        };
        let chosen = at == palette.selected;
        let content = row![
            line(entry.label.as_str()).width(Length::Fill),
            line(entry.keys.as_str()).color(tokens.text_secondary),
        ]
        .spacing(12);
        let cell = container(content)
            .padding([4, 8])
            .width(Length::Fill)
            .style(move |_| theme::row(tokens, chosen, true, false));
        body = body.push(mouse_area(cell).on_press(Message::Palette(PaletteMsg::Pick(at))));
    }
    let card = container(body)
        .padding(12)
        .width(CARD_WIDTH)
        .style(move |_| theme::dialog(tokens));
    let placed = column![
        Space::new().height(48.0),
        container(card).center_x(Length::Fill)
    ];
    opaque(
        container(placed)
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| theme::backdrop(tokens)),
    )
}
