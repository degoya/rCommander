//! The left side: the filter, the tree's rows with the cursor, the name field of a new or
//! renamed node, the delete question, and the tree's buttons. A row is one line of a fixed
//! height: the name is cut at its column's end, the host in the middle (the whole value in a
//! tooltip), and a list too narrow for both shows the names alone.

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, button, column, container, mouse_area, responsive, row, scrollable},
};
use rc_engine::TreeItem;

use super::{
    FILTER_FIELD, NAME_FIELD, TREE_SCROLL_ID, TREE_STOP_ID, cut, field, hint, label, path_cell,
    wrap,
};
use crate::{
    app::{
        Message,
        connections::{
            ConnMsg, ConnectionManager, Focus, NameFor, Pending, RdmImportMsg, import::ImportMsg,
            tree::Row,
        },
    },
    i18n::t,
    theme::{self, Tokens, focus, icon::Icon, layout},
};

/// Pixels a level of the tree indents.
const INDENT: f32 = 16.0;
/// Below this width the list shows the names without the hosts.
const HOST_MIN_WIDTH: f32 = 360.0;

pub(super) fn side<'a>(
    manager: &'a ConnectionManager,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let filter = field(t!("connections.filter"), &manager.tree.filter)
        .id(FILTER_FIELD)
        .on_input(|text| wrap(ConnMsg::Filter(text)));
    let active = manager.focus == Focus::Tree;
    // The list's width decides whether the hosts fit beside the names.
    let list = responsive(move |size| {
        let rows = rows_view(manager, size.width >= HOST_MIN_WIDTH, tokens);
        container(
            scrollable(rows)
                .id(TREE_SCROLL_ID)
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, active))
        .into()
    });
    // The tree is one stop of the ring; the manager moves its cursor (`keys.rs`).
    let list = focus::Stop::choice(list, TREE_STOP_ID, None, None).id(TREE_STOP_ID);
    let mut side = column![filter, Element::from(list)].spacing(8);
    if let Some(Pending::Delete {
        item,
        folders,
        connections,
    }) = &manager.pending
    {
        side = side.push(delete_question(
            manager,
            *item,
            *folders,
            *connections,
            tokens,
        ));
    }
    let buttons = iced::widget::Row::with_children(
        buttons(manager.tree.offer_credentials)
            .into_iter()
            .map(|(icon, title, message)| small(icon, title, message)),
    )
    .spacing(6)
    .wrap()
    .vertical_spacing(6);
    side.push(buttons)
        .push(hint(t!("connections.keys"), tokens))
        .into()
}

/// The buttons under the tree: their texts and messages; a new shared credential only while
/// they are offered. The Total Commander import is not one; it takes more than connections, and
/// the settings open it (RC-050-10).
fn buttons(credentials: bool) -> Vec<(Option<Icon>, &'static str, ConnMsg)> {
    let mut buttons = vec![
        (
            Some(Icon::Add),
            t!("connections.new_connection"),
            ConnMsg::NewConnection,
        ),
        (
            Some(Icon::NewFolder),
            t!("connections.new_folder"),
            ConnMsg::NewFolder,
        ),
        (
            Some(Icon::Copy),
            t!("connections.duplicate"),
            ConnMsg::Duplicate,
        ),
    ];
    if credentials {
        buttons.push((
            Some(Icon::Add),
            t!("connections.new_credential"),
            ConnMsg::NewCredential,
        ));
    }
    buttons.extend([
        (
            Some(Icon::Import),
            t!("connections.import"),
            ConnMsg::Import(ImportMsg::Open),
        ),
        (
            Some(Icon::Import),
            t!("rdm_import.open"),
            ConnMsg::RdmImport(RdmImportMsg::Open),
        ),
        (None, t!("connections.login_scripts"), ConnMsg::LoginScripts),
    ]);
    buttons
}

/// The tree's rows, the hosts beside the names when `hosts`, and the name field of a new folder.
fn rows_view<'a>(
    manager: &'a ConnectionManager,
    hosts: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let rows = manager.tree.rows();
    let cursor = manager.tree.cursor_index(&rows);
    let active = manager.focus == Focus::Tree;
    let mut list = Column::new().spacing(1);
    if rows.is_empty() {
        list = list.push(hint(
            if manager.tree.filter.trim().is_empty() {
                t!("connections.empty")
            } else {
                t!("connections.no_match")
            },
            tokens,
        ));
    }
    for (at, tree_row) in rows.iter().enumerate() {
        list = list.push(row_view(
            manager,
            tree_row,
            (cursor == Some(at), active, hosts, theme::alternate(at)),
            tokens,
        ));
        if let Some(Pending::Name {
            target: NameFor::NewFolder(Some(parent)),
            text,
        }) = &manager.pending
            && tree_row.item == TreeItem::Folder(*parent)
        {
            list = list.push(name_field(text, tree_row.depth + 1));
        }
    }
    if let Some(Pending::Name {
        target: NameFor::NewFolder(None),
        text,
    }) = &manager.pending
    {
        list = list.push(name_field(text, 0));
    }
    list.into()
}

/// One row: indent, folder sign, star or a credential's diamond, name and host (a credential's
/// user); it takes clicks and drags. `state`:
/// whether the cursor is on it, whether the tree has the keys, whether the host is drawn,
/// whether it is striped.
fn row_view<'a>(
    manager: &'a ConnectionManager,
    tree_row: &Row,
    state: (bool, bool, bool, bool),
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (cursor, active, hosts, alternate) = state;
    let item = tree_row.item;
    if let Some(Pending::Name {
        target: NameFor::Rename(renamed),
        text,
    }) = &manager.pending
        && *renamed == item
    {
        return name_field(text, tree_row.depth);
    }
    let sign = match item {
        TreeItem::Folder(_) if tree_row.open => "\u{25BE}",
        TreeItem::Folder(_) => "\u{25B8}",
        TreeItem::Connection(id) if manager.favourites.contains(&id) => "\u{2605}",
        TreeItem::Connection(_) => "\u{00B7}",
        TreeItem::Credential(_) => "\u{25C6}",
    };
    let cut_out = manager.clipboard == Some(item);
    let font = if matches!(item, TreeItem::Folder(_)) {
        theme::UI_FONT_STRONG
    } else {
        theme::UI_FONT
    };
    let name = cut(tree_row.name, font, None, 3, tokens);
    let mut line = row![
        Space::new().width(INDENT * tree_row.depth as f32),
        label(sign).width(16).color(tokens.accent),
        name,
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    if hosts {
        line = line.push(path_cell(tree_row.detail, 2, true, tokens));
    }
    let shown = container(line)
        .padding([0, 6])
        .width(Length::Fill)
        .height(theme::ROW_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::striped_row(tokens, cursor, active, cut_out, alternate));
    mouse_area(shown)
        .on_press(wrap(ConnMsg::Press(item)))
        .on_release(wrap(ConnMsg::Release(item)))
        .on_double_click(wrap(ConnMsg::Activate(item)))
        .into()
}

/// The field a new or changed name is typed into; Enter takes it, Escape drops it.
fn name_field<'a>(text: &str, depth: usize) -> Element<'a, Message> {
    row![
        Space::new().width(INDENT * depth as f32 + 20.0),
        field(t!("connections.name"), text)
            .id(NAME_FIELD)
            .on_input(|text| wrap(ConnMsg::NameInput(text)))
            .on_submit(wrap(ConnMsg::NameSubmit)),
    ]
    .into()
}

/// "Delete ...?" with what goes with it, Yes and No.
fn delete_question<'a>(
    manager: &'a ConnectionManager,
    item: TreeItem,
    folders: u64,
    connections: u64,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let name = manager.tree.name_of(item);
    let question = match item {
        TreeItem::Folder(_) => t!(
            "connections.delete_folder",
            name = name,
            folders = folders.saturating_sub(1),
            connections = connections
        ),
        TreeItem::Connection(_) => t!("connections.delete_connection", name = name),
        TreeItem::Credential(_) => t!(
            "connections.delete_credential",
            name = name,
            connections = connections
        ),
    };
    container(
        column![
            label(question),
            row![
                Space::new().width(Length::Fill),
                small(None, t!("connections.yes"), ConnMsg::Confirm(true)),
                small(None, t!("connections.no"), ConnMsg::Confirm(false)),
            ]
            .spacing(6),
        ]
        .spacing(6),
    )
    .padding(8)
    .style(move |_| theme::panel(tokens, true))
    .into()
}

/// A small button with its icon, a stop of the focus ring with the access key its title marks;
/// its title stays on one line, a row of them wraps instead.
fn small<'a>(icon: Option<Icon>, title: &'static str, message: ConnMsg) -> Element<'a, Message> {
    let message = wrap(message);
    let face = button(layout::marked_icon_button(icon, title))
        .padding([4, 10])
        .style(button::secondary)
        .on_press(message.clone());
    layout::keyed(focus::Stop::button(face, title, Some(message)), title)
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;
