//! The tabs over a page of a dialog (RC-100-20), after WinUI's pivot: a row of tab labels above
//! the page's content - the chosen one in the text colour with a 2-px accent bar under it, the
//! others in `text_secondary`, filled while the pointer is on them - over a 1-px `stroke` line
//! across the page. The row is one stop of the focus ring (`theme::focus::pick`): the arrows
//! choose the tab before and after it, a click the tab clicked. Which tab is chosen is the
//! caller's; the strip only sends `on_select`. `docs/design.md`, Tabs.

use iced::{
    Background, Border, Element, Length,
    widget::{Row, Space, button, column, container, text},
};

use super::{TEXT_SIZE, Tokens, UI_FONT, divider, focus};

/// The height of the accent bar under the chosen tab.
pub const UNDERLINE: f32 = 2.0;
/// The space between two tabs.
const SPACING: f32 = 2.0;

/// The tabs `tabs` - each its value and its label - with `chosen` marked, as one stop of the
/// ring named `name`; a click on a tab or an arrow sends `on_select` with the tab chosen.
pub fn strip<'a, T, Message>(
    tabs: &[(T, &'static str)],
    chosen: T,
    name: &str,
    on_select: impl Fn(T) -> Message,
    tokens: &'static Tokens,
) -> Element<'a, Message>
where
    T: Copy + PartialEq,
    Message: Clone + 'a,
{
    let entries: Vec<Element<'a, Message>> = tabs
        .iter()
        .map(|&(tab, title)| {
            let on = tab == chosen;
            let face = button(text(title).size(TEXT_SIZE).font(UI_FONT))
                .padding([6, 12])
                .style(move |_, status| entry(tokens, on, status))
                .on_press(on_select(tab));
            let bar = container(Space::new().width(Length::Fill).height(UNDERLINE))
                .style(move |_| underline(tokens, on));
            column![face, bar].into()
        })
        .collect();
    let options: Vec<T> = tabs.iter().map(|(tab, _)| *tab).collect();
    let row = focus::pick(
        Row::with_children(entries).spacing(SPACING),
        name,
        &options,
        Some(&chosen),
        on_select,
    );
    let line =
        container(Space::new().width(Length::Fill).height(1.0)).style(move |_| divider(tokens));
    column![row, line].width(Length::Fill).into()
}

/// The tab after `chosen` in `tabs` (`back`: before it), round from the last to the first, as
/// Ctrl+Tab and Ctrl+Shift+Tab turn a Windows dialog's tabs; the first when `chosen` is none
/// of them.
pub fn turn<T: Copy + PartialEq>(tabs: &[T], chosen: T, back: bool) -> T {
    let count = tabs.len();
    let Some(at) = tabs.iter().position(|tab| *tab == chosen) else {
        return tabs.first().copied().unwrap_or(chosen);
    };
    let next = if back {
        (at + count - 1) % count
    } else {
        (at + 1) % count
    };
    tabs.get(next).copied().unwrap_or(chosen)
}

/// A tab's face: the chosen one in the text colour and never filled, the others in
/// `text_secondary` and filled while the pointer is on them.
pub fn entry(tokens: &Tokens, chosen: bool, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered | button::Status::Pressed);
    button::Style {
        background: (hovered && !chosen).then_some(Background::Color(tokens.cursor_fill_inactive)),
        text_color: if chosen {
            tokens.text
        } else {
            tokens.text_secondary
        },
        border: Border::default().rounded(4.0),
        ..button::Style::default()
    }
}

/// The bar under a tab: the accent under the chosen one, nothing under the others.
pub fn underline(tokens: &Tokens, chosen: bool) -> container::Style {
    container::Style {
        background: chosen.then_some(Background::Color(tokens.accent)),
        ..container::Style::default()
    }
}

#[cfg(test)]
#[path = "page_tabs_tests.rs"]
mod tests;
