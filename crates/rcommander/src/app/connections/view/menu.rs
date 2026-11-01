//! Drawing the tree's context menu (RC-100-63, `tree_menu.rs`): a card of entries, each with its
//! key on the right as in a Windows menu, an entry that cannot run greyed out; over the tree's
//! side, so that a click beside it closes it. The look is the file list's own menu's
//! (`app/context_menu/view.rs`).

use iced::{
    Alignment, Element, Length,
    mouse::Interaction,
    widget::{column, container, mouse_area, pin, row, stack},
};

use super::wrap;
use crate::{
    app::{
        Message,
        connections::{
            ConnMsg, ConnectionManager,
            tree_edit::{TreeAction, TreeMsg},
            tree_menu::MENU_WIDTH,
        },
        context_menu::keys,
    },
    keymap,
    theme::{self, Tokens, fit::line},
};

/// `side` with the open menu over it, once its place is known.
pub(super) fn over<'a>(
    manager: &'a ConnectionManager,
    side: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let Some((cursor, at)) = manager
        .edit
        .menu
        .as_ref()
        .and_then(|menu| menu.at.map(|at| (menu.cursor, at)))
    else {
        return side;
    };
    let entries = TreeAction::MENU
        .into_iter()
        .enumerate()
        .map(|(index, action)| entry(action, index, index == cursor, manager.can(action), tokens));
    let card = container(column(entries).padding(4))
        .width(MENU_WIDTH)
        .style(move |_| theme::dialog(tokens));
    let close = wrap(ConnMsg::Tree(TreeMsg::MenuClose));
    // focus-ring: pointer only (the layer that closes the menu)
    let layer = mouse_area(pin(card).x(at.x).y(at.y))
        .on_press(close.clone())
        .on_right_press(close);
    stack![side, layer].into()
}

/// One entry: its name, its key (while the menus show keys) on the right; filled while the
/// keyboard or the pointer is on it, dimmed when it cannot run.
fn entry<'a>(
    action: TreeAction,
    index: usize,
    cursor: bool,
    enabled: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let key = if keys::shown() {
        keymap::label_text(action.keys())
    } else {
        String::new()
    };
    let mut name = line(action.label()).width(Length::Fill);
    if !enabled {
        name = name.color(tokens.text_secondary);
    }
    let face = container(
        row![name, line(key).color(tokens.text_secondary)]
            .spacing(16)
            .padding([0, 10])
            .align_y(Alignment::Center),
    )
    .height(theme::ROW_HEIGHT)
    .width(Length::Fill)
    .align_y(Alignment::Center)
    .style(move |_| theme::row(tokens, cursor && enabled, true, false));
    // focus-ring: pointer only (an entry of the menu, worked by the menu's own keys)
    let mut area = mouse_area(face)
        .on_enter(wrap(ConnMsg::Tree(TreeMsg::MenuHover(index))))
        .interaction(Interaction::Pointer);
    if enabled {
        area = area.on_press(wrap(ConnMsg::Tree(TreeMsg::Action(action))));
    }
    area.into()
}
