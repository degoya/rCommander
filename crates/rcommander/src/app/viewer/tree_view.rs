//! Drawing a document's tree and its list of views (RC-100-10). The tree draws only the rows on
//! screen, each in the table's row with its sign (unfolds, folded, a leaf, loading), the label
//! and the value shortened in the middle with the whole of it in a tooltip (`docs/design.md`,
//! "Text never leaves its container"). The list of views sits in the title row while the
//! document offers more than one; it is the Lister's menu entry beside the key `7`.

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, center, container, mouse_area, pick_list, row, sensor},
};

use super::{
    ViewerMessage,
    document::{DocView, DocumentMessage},
    lister::Lister,
    wrap,
};
use crate::{
    app::Message,
    format,
    i18n::t,
    theme::{self, Tokens, fit, focus},
};

/// The indent of a level of the tree.
const INDENT: f32 = 16.0;

/// The tree of `lister`'s document filling the body.
pub fn view<'a>(lister: &'a Lister, tokens: &'static Tokens) -> Element<'a, Message> {
    let id = lister.id;
    let Some(document) = &lister.document else {
        return Space::new().into();
    };
    let tree = &document.tree;
    if tree.loading() {
        return center(fit::line(t!("viewer.loading"))).into();
    }
    if tree.is_empty() {
        return center(fit::line(t!("viewer.tree_empty"))).into();
    }
    let mut rows = Column::new().spacing(0);
    for (at, shown_row) in tree.visible().iter().enumerate() {
        let index = tree.top + at;
        let Some(shown) = tree.shown(*shown_row) else {
            continue;
        };
        let sign = match shown.open {
            _ if shown.loading => "\u{2026}",
            Some(true) => "\u{25BE}",
            Some(false) => "\u{25B8}",
            None => "\u{00B7}",
        };
        let mut line = row![
            Space::new().width(INDENT * shown_row.depth as f32),
            fit::line(sign).width(16).color(tokens.accent),
            fit::path_cell(shown.label.to_owned(), 3, false, tokens),
        ]
        .spacing(4)
        .align_y(Alignment::Center);
        if let Some(value) = shown.value {
            line = line.push(fit::path_cell(value.to_owned(), 2, true, tokens));
        }
        let (cursor, alternate) = (index == tree.cursor, theme::alternate(index));
        let cell = container(line)
            .padding([0, 6])
            .width(Length::Fill)
            .height(theme::ROW_HEIGHT)
            .align_y(Alignment::Center)
            .clip(true)
            .style(move |_| theme::striped_row(tokens, cursor, true, false, alternate));
        // focus-ring: pointer only (a row of the tree, which the viewer's arrows move through)
        rows = rows.push(mouse_area(cell).on_press(wrap(ViewerMessage::Document(
            id,
            DocumentMessage::TreeClick(index),
        ))));
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
        let rows = i32::try_from(rows).unwrap_or(0);
        tree_message(id, DocumentMessage::TreeScroll(rows))
    });
    sensor(body)
        .on_show(move |size| tree_message(id, DocumentMessage::TreeViewport(size.height)))
        .on_resize(move |size| tree_message(id, DocumentMessage::TreeViewport(size.height)))
        .into()
}

fn tree_message(id: u64, message: DocumentMessage) -> Message {
    wrap(ViewerMessage::Document(id, message))
}

/// The title row: the file's name, and the list of a document's views when it offers several.
pub fn title<'a>(lister: &'a Lister, name: Element<'a, Message>) -> Element<'a, Message> {
    let Some(document) = lister.document.as_ref().filter(|d| d.views().len() > 1) else {
        return name;
    };
    let id = lister.id;
    let views = document.views();
    let current = document.view();
    let pick = move |view: DocView| wrap(ViewerMessage::Document(id, DocumentMessage::View(view)));
    let list = pick_list(views, Some(current), pick)
        .text_size(theme::TEXT_SIZE)
        .padding([2, 8]);
    let list = focus::pick(list, t!("viewer.views"), views, Some(&current), pick);
    row![container(name).width(Length::Fill).clip(true), list]
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
}

/// The status line of the tree: how many nodes it holds, and when it stopped growing.
pub fn status(lister: &Lister) -> Option<String> {
    let document = lister.document.as_ref().filter(|_| lister.tree_shown())?;
    let nodes = format::count(document.tree.len() as u64);
    Some(if document.tree.full {
        t!("viewer.status_tree_full", nodes = nodes)
    } else {
        t!("viewer.status_tree", nodes = nodes)
    })
}

/// The help line's keys: with `7` while a document offers several views.
pub fn hints(lister: &Lister) -> &'static str {
    if lister
        .document
        .as_ref()
        .is_some_and(|document| document.views().len() > 1)
    {
        t!("viewer.hints_views")
    } else {
        t!("viewer.hints")
    }
}
