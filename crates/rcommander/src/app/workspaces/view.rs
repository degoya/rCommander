//! Drawing the window (RC-100-68): the list on the left - the filter, the workspaces with the
//! cursor and the active one marked, a name being typed, the delete question, the list's buttons
//! -, the form on the right (`view/form.rs`, each panel `view/panel.rs`), the note and the buttons
//! below in one row. A large dialog: in a window of its own while `appearance.dialog_windows` is
//! on, else a card over the main window.

mod form;
mod panel;

use iced::{
    Alignment, Element, Length,
    widget::{
        Column, Row, Space, button, column, container, mouse_area, row, scrollable, text,
        text_input,
    },
};

use super::{NameFor, Pending, Shown, WorkspaceWindow, WsMsg, keys::Focus, list};
use crate::{
    app::{Message, palette::workspaces::WorkspaceRow},
    i18n::t,
    theme::{self, Tokens, fit, focus, icon::Icon, layout},
};

/// The list's filter: the keyboard starts here.
pub const FILTER_FIELD: &str = "rcommander.workspaces.filter";
/// The list as one stop of the focus ring.
pub const LIST_STOP_ID: &str = "rcommander.workspaces.list";
/// The name of a new or renamed workspace.
pub const NAME_FIELD: &str = "rcommander.workspaces.name";

fn wrap(message: WsMsg) -> Message {
    Message::Workspaces(message)
}

/// A one-line text field of the dialog's look.
fn field<'a>(placeholder: &str, value: &str) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8])
}

/// The window over the main window, as a card.
pub fn layer<'a>(
    window: &'a WorkspaceWindow,
    shown: Shown,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    theme::large_dialog(
        Message::dialog_head(fit::label(t!("workspaces.title")).size(18)),
        body(window, shown, tokens),
        tokens,
    )
}

/// The list, the form and the one row of buttons: the card's content over the main window, the
/// whole of its own window.
pub fn body<'a>(
    window: &'a WorkspaceWindow,
    shown: Shown,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let key = shown.key.clone();
    let main = row![
        // The list and the form share the width, 2:3.
        container(list_side(window, &shown.rows, tokens))
            .width(Length::FillPortion(2))
            .height(Length::Fill),
        container(form::side(window, &shown, key, tokens))
            .width(Length::FillPortion(3))
            .height(Length::Fill),
    ]
    .spacing(16)
    .height(Length::Fill);
    let note: Element<'a, Message> = match &window.note {
        Some((note, error)) => {
            let color = if *error {
                theme::DANGER
            } else {
                tokens.text_secondary
            };
            fit::label(note.as_str())
                .color(color)
                .wrapping(text::Wrapping::WordOrGlyph)
                .into()
        }
        None => Space::new().into(),
    };
    let chosen = window.cursor.is_some();
    let idle = !window.busy;
    let buttons = Row::new()
        .push(layout::secondary_button(
            t!("workspaces.capture"),
            (chosen && idle).then(|| wrap(WsMsg::Capture)),
        ))
        .push(container(note).width(Length::Fill))
        .push(layout::primary_icon_button(
            Icon::Confirm,
            t!("workspaces.apply"),
            (chosen && idle).then(|| wrap(WsMsg::Apply)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Save,
            t!("workspaces.save"),
            window
                .form
                .as_ref()
                .filter(|form| form.dirty && idle)
                .map(|_| wrap(WsMsg::Save)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Close,
            t!("workspaces.close"),
            Some(wrap(WsMsg::Close)),
        ))
        .spacing(12)
        .align_y(Alignment::Center);
    column![main, buttons].spacing(12).into()
}

/// The left side: the filter, the rows, the question or the name field, the buttons, the keys.
fn list_side<'a>(
    window: &'a WorkspaceWindow,
    rows: &[WorkspaceRow],
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let filter = field(t!("workspaces.filter"), &window.filter)
        .id(FILTER_FIELD)
        .on_input(|text| wrap(WsMsg::Filter(text)));
    let active = matches!(window.focus, Focus::List | Focus::Filter);
    let shown = list::shown(rows, &window.filter);
    let mut lines = Column::new().spacing(1);
    if shown.is_empty() {
        let empty = if rows.is_empty() {
            t!("workspaces.empty")
        } else {
            t!("workspaces.no_match")
        };
        lines = lines.push(fit::hint(empty, tokens));
    }
    for (at, id) in shown.iter().enumerate() {
        let Some(workspace) = rows.iter().find(|row| row.id == *id) else {
            continue;
        };
        let renaming = matches!(
            &window.pending,
            Some(Pending::Name { target: NameFor::Rename(renamed), .. }) if renamed == id
        );
        if renaming {
            lines = lines.push(name_field(window));
            continue;
        }
        let cursor = window.cursor == Some(*id);
        lines = lines.push(row_view(
            workspace,
            (cursor, active, theme::alternate(at)),
            tokens,
        ));
    }
    if matches!(
        &window.pending,
        Some(Pending::Name {
            target: NameFor::New,
            ..
        })
    ) {
        lines = lines.push(name_field(window));
    }
    let list = container(scrollable(lines).width(Length::Fill).height(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| theme::panel(tokens, active));
    // The list is one stop of the ring; the window moves its cursor (`keys.rs`).
    let list = focus::Stop::choice(list, LIST_STOP_ID, None, None).id(LIST_STOP_ID);
    let mut side = column![filter, Element::from(list)].spacing(8);
    if let Some(Pending::Delete(id)) = &window.pending {
        let name = rows
            .iter()
            .find(|row| row.id == *id)
            .map_or_else(String::new, |row| row.name.clone());
        side = side.push(delete_question(name, tokens));
    }
    let chosen = window.cursor.is_some();
    let buttons = Row::new()
        .push(small(
            Some(Icon::Add),
            t!("workspaces.new"),
            Some(WsMsg::New),
        ))
        .push(small(
            Some(Icon::Rename),
            t!("workspaces.rename"),
            chosen.then_some(WsMsg::Rename),
        ))
        .push(small(
            Some(Icon::Delete),
            t!("workspaces.delete"),
            chosen.then_some(WsMsg::Delete),
        ))
        .push(layout::glyph_button(
            Icon::MoveUp,
            t!("workspaces.move_up"),
            chosen.then(|| wrap(WsMsg::Move(true))),
            tokens,
        ))
        .push(layout::glyph_button(
            Icon::MoveDown,
            t!("workspaces.move_down"),
            chosen.then(|| wrap(WsMsg::Move(false))),
            tokens,
        ))
        .spacing(6)
        .wrap()
        .vertical_spacing(6);
    side.push(buttons)
        .push(fit::help_line(t!("workspaces.keys"), tokens))
        .into()
}

/// One row: the active mark and the name; a click puts the cursor on it, a double click applies
/// it. `state`: the cursor is on it, the list has the keys, it is striped.
fn row_view<'a>(
    workspace: &WorkspaceRow,
    state: (bool, bool, bool),
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (cursor, active, alternate) = state;
    let mark = if workspace.active { "\u{25CF}" } else { "" };
    let line = row![
        fit::label(mark).width(16).color(tokens.accent),
        fit::cut(workspace.name.clone(), theme::UI_FONT, None, 1, tokens),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let shown = container(line)
        .padding([0, 6])
        .width(Length::Fill)
        .height(theme::ROW_HEIGHT)
        .align_y(Alignment::Center)
        .clip(true)
        .style(move |_| theme::striped_row(tokens, cursor, active, false, alternate));
    let id = workspace.id;
    // focus-ring: pointer only (a row of the list, which is one stop of the ring)
    mouse_area(shown)
        .on_press(wrap(WsMsg::Select(id)))
        .on_double_click(wrap(WsMsg::Activate(id)))
        .into()
}

/// The field a new or changed name is typed into; Enter takes it, Escape drops it.
fn name_field(window: &WorkspaceWindow) -> Element<'_, Message> {
    let text = match &window.pending {
        Some(Pending::Name { text, .. }) => text.as_str(),
        _ => "",
    };
    field(t!("workspaces.name_placeholder"), text)
        .id(NAME_FIELD)
        .on_input(|typed| wrap(WsMsg::NameInput(typed)))
        .on_submit(wrap(WsMsg::NameSubmit))
        .into()
}

/// "Delete the workspace ...?", Delete and Keep.
fn delete_question<'a>(name: String, tokens: &'static Tokens) -> Element<'a, Message> {
    container(
        column![
            fit::label(t!("workspaces.delete_question", name = name)),
            row![
                Space::new().width(Length::Fill),
                small(None, t!("workspaces.yes"), Some(WsMsg::Confirm(true))),
                small(None, t!("workspaces.no"), Some(WsMsg::Confirm(false))),
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
/// `None` draws it disabled.
fn small<'a>(
    icon: Option<Icon>,
    title: &'static str,
    message: Option<WsMsg>,
) -> Element<'a, Message> {
    let message = message.map(wrap);
    let face = button(layout::marked_icon_button(icon, title))
        .padding([4, 10])
        .style(button::secondary)
        .on_press_maybe(message.clone());
    layout::keyed(focus::Stop::button(face, title, message), title)
}
