//! Drawing the folder trees (RC-100-88). A tree draws only the rows on screen, each in the
//! list's row with its sign (unfolded, folded, without sub-folders, being read, failed) and the
//! folder's name shortened in the middle with the whole of it in a tooltip (`docs/design.md`,
//! "Text never leaves its container"); a folder whose level failed carries the error in its
//! tooltip. The tree in a panel's place fills the panel's body; the separate trees stand left of
//! each panel's body, under its path bar and over its status line as in Total Commander, or as
//! one card left of both panels; their right edge is dragged as the divider is (`resize.rs`).

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, container, mouse_area, row, sensor},
};
use rc_engine::{PanelId, PanelView, tree::SeparateTree};

use super::{DirTree, TreeId, TreeMsg, model::Sign};
use crate::{
    app::{App, Message, resize},
    i18n::{error_text, t},
    theme::{self, Tokens, fit},
};

/// The indent of a level of the tree.
const INDENT: f32 = 16.0;

fn wrap(message: TreeMsg) -> Message {
    Message::Tree(message)
}

/// The rows of `tree` on screen, filling the room they get; the cursor's row in the active
/// colour while the tree has the keyboard (`focused`).
pub fn tree_list<'a>(
    id: TreeId,
    tree: &'a DirTree,
    focused: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut rows = Column::new().spacing(0);
    for (at, shown_row) in tree.visible().iter().enumerate() {
        let index = tree.top + at;
        let Some(shown) = tree.shown(*shown_row) else {
            continue;
        };
        let glyph = match shown.sign {
            Sign::Open => "\u{25BE}",
            Sign::Closed => "\u{25B8}",
            Sign::Leaf => "\u{00B7}",
            Sign::Loading => "\u{2026}",
            Sign::Failed => "!",
        };
        // focus-ring: pointer only (a row's sign, which Right and Left work from the keyboard)
        let sign = mouse_area(fit::line(glyph).width(16).color(tokens.accent))
            .on_press(wrap(TreeMsg::Toggle(id, index)));
        let label = if shown.truncated {
            format!("{} {}", shown.name, t!("tree.truncated"))
        } else {
            shown.name.to_owned()
        };
        let mut name = fit::path_cell(label, 1, false, tokens);
        if let Some(code) = shown.error {
            name = fit::tip(name, error_text(code), tokens);
        }
        let line = row![
            Space::new().width(INDENT * shown_row.depth as f32),
            sign,
            name,
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        let (cursor, alternate) = (index == tree.cursor, theme::alternate(index));
        let cell = container(line)
            .padding([0, 6])
            .width(Length::Fill)
            .height(theme::ROW_HEIGHT)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| theme::striped_row(tokens, cursor, focused, false, alternate));
        // focus-ring: pointer only (a row of the tree, which the arrows move through)
        rows = rows.push(
            mouse_area(cell)
                .on_press(wrap(TreeMsg::Click(id, index)))
                .on_double_click(wrap(TreeMsg::Open(id, index))),
        );
    }
    if tree.full {
        rows = rows.push(fit::line(t!("tree.full")).color(tokens.text_secondary));
    }
    let body = mouse_area(
        container(rows)
            .width(Length::Fill)
            .height(Length::Fill)
            .clip(true),
    )
    .on_scroll(move |delta| {
        let rows = theme::wheel::rows(delta, theme::ROW_HEIGHT);
        // A few rows of wheel: the conversion does not fail.
        wrap(TreeMsg::Scroll(id, i32::try_from(rows).unwrap_or(0)))
    });
    sensor(body)
        .on_show(move |size| wrap(TreeMsg::Viewport(id, size.height)))
        .on_resize(move |size| wrap(TreeMsg::Viewport(id, size.height)))
        .into()
}

/// The tree in `panel`'s place, while the panel shows its tree.
pub(in crate::app) fn panel_body<'a>(
    app: &'a App,
    panel: PanelId,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    if app.pane(panel).state.view != PanelView::Tree {
        return None;
    }
    let kept = app.tree.panels[panel.index()].as_ref()?;
    let focused = panel == app.active && !app.tree.focus;
    Some(tree_list(TreeId::Panel(panel), &kept.tree, focused, tokens))
}

/// `body` of `panel` with the panel's separate tree on its left while there is one beside each
/// panel; `body` alone otherwise.
pub(in crate::app) fn separate_beside<'a>(
    app: &'a App,
    panel: PanelId,
    body: Element<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    if app.tree.mode != SeparateTree::Two {
        return body;
    }
    let Some(kept) = app.tree.separate[panel.index()].as_ref() else {
        return body;
    };
    let focused = app.tree.focus && panel == app.active;
    let tree = container(tree_list(
        TreeId::Separate(panel),
        &kept.tree,
        focused,
        tokens,
    ))
    .width(app.resize.tree_width())
    .height(Length::Fill)
    .clip(true);
    row![tree, resize::tree_grip(), body]
        .height(Length::Fill)
        .into()
}

/// The one separate tree left of both panels, the active panel's, as a card of its own, with
/// its edge; `None` while the trees are not one.
pub(in crate::app) fn separate_one<'a>(
    app: &'a App,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    if app.tree.mode != SeparateTree::One {
        return None;
    }
    let panel = app.active;
    let kept = app.tree.separate[panel.index()].as_ref()?;
    let focused = app.tree.focus;
    let card = container(tree_list(
        TreeId::Separate(panel),
        &kept.tree,
        focused,
        tokens,
    ))
    .padding(1)
    .width(app.resize.tree_width())
    .height(Length::Fill)
    .clip(true)
    .style(move |_| theme::panel(tokens, focused));
    Some(row![card, resize::tree_grip()].height(Length::Fill).into())
}
