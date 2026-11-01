//! The toolbar above the tree (RC-100-63): rename, up, down, outdent, indent, the drive-bar star
//! and delete for the node at the cursor, each an icon with its name and key in a tooltip, a
//! stop of the focus ring, greyed out while it cannot run. And the pane a folder at the cursor
//! shows beside the tree: its name and whether the drive bar shows it as a button.

use iced::{
    Element, Pixels,
    widget::{Row, button, column, text},
};
use rc_engine::FolderId;

use super::{hint, label, wrap};
use crate::{
    app::{
        Message,
        connections::{
            ConnMsg, ConnectionManager,
            tree_edit::{TreeAction, TreeMsg},
        },
    },
    i18n::t,
    keymap,
    theme::{self, Tokens, fit, focus, icon, layout},
};

/// The toolbar: one button per action of [`TreeAction::TOOLBAR`].
pub(super) fn bar<'a>(
    manager: &'a ConnectionManager,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    Row::with_children(
        TreeAction::TOOLBAR
            .into_iter()
            .map(|action| tool(action, manager.can(action), tokens)),
    )
    .spacing(4)
    .wrap()
    .vertical_spacing(4)
    .into()
}

/// One button: the action's icon, its name and key in the tooltip ("Move up (Ctrl+Up)").
fn tool<'a>(action: TreeAction, enabled: bool, tokens: &'static Tokens) -> Element<'a, Message> {
    let line_height = text::LineHeight::default().to_absolute(Pixels(theme::TEXT_SIZE));
    let glyph = text(action.icon().glyph())
        .font(icon::FONT)
        .size(icon::SIZE)
        .line_height(line_height);
    let on_press = enabled.then(|| wrap(ConnMsg::Tree(TreeMsg::Action(action))));
    let control = button(glyph)
        .padding([4, 8])
        .style(button::secondary)
        .on_press_maybe(on_press.clone());
    let tip = format!("{} ({})", action.label(), keymap::label_text(action.keys()));
    fit::tip(
        focus::Stop::button(control, action.label(), on_press),
        tip,
        tokens,
    )
}

/// A folder at the cursor: its name, and the switch that shows it as a button of the drive bar
/// (Space in the tree does the same).
pub(super) fn folder_pane<'a>(
    manager: &'a ConnectionManager,
    id: FolderId,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let name = manager.tree.name_of(rc_engine::TreeItem::Folder(id));
    let star = |_: bool| wrap(ConnMsg::Tree(TreeMsg::Action(TreeAction::Star)));
    column![
        label(name).font(theme::UI_FONT_STRONG),
        layout::check(
            t!("connections.folder_on_bar"),
            manager.tree.on_bar(id),
            true,
            star
        ),
        hint(t!("connections.folder_on_bar_help"), tokens),
    ]
    .spacing(8)
    .padding(8)
    .into()
}

#[cfg(test)]
#[path = "toolbar_tests.rs"]
mod tests;
