//! Drawing the window (RC-100-86): the two tabs over the page, on the left the own commands with
//! their filter (`view/own.rs`) or the Start menu's tree (`view/start.rs`) with the list's buttons,
//! on the right the form of the row at the cursor - the command's fields shared by both
//! (`view/command.rs`) -, the note and the buttons below in one row; or, while "Choose
//! symbol..." is open, the symbol picker's page (`app/symbol_picker`, RC-110-10). A large dialog: in a window
//! of its own while `appearance.dialog_windows` is on, else a card over the main window.

mod command;
mod own;
mod start;

/// The command's fields and the form's building blocks, for the internal associations' and the
/// button bars' windows (RC-100-91, RC-100-87).
pub(in crate::app) use self::command::{
    Labeled, choice, field, fields, fields as command_fields, help, labeled,
};

use iced::{
    Alignment, Element, Length,
    widget::{Row, Space, column, container, row, text},
};
use rc_engine::{StartItem, StartItemKind, UserCommand, start_key_name};

use super::{CommandsWindow, Tab, UcMsg, known};
use crate::{
    app::{Message, symbol_picker},
    i18n::t,
    theme::{self, Tokens, fit, icon::Icon, layout, page_tabs},
};

/// The own commands' filter: the keyboard starts here on their tab.
pub const FILTER_FIELD: &str = "rcommander.user_commands.filter";
/// The own commands' list as one stop of the focus ring.
pub const LIST_STOP_ID: &str = "rcommander.user_commands.list";
/// The Start menu's tree as one stop of the focus ring: the keyboard starts here on its tab.
pub const TREE_STOP_ID: &str = "rcommander.user_commands.tree";
/// The own command's name in its form.
pub const NAME_FIELD: &str = "rcommander.user_commands.name";
/// The Start menu row's title in its form.
pub const TITLE_FIELD: &str = "rcommander.user_commands.title";

/// What the window draws besides its own state, read from the app when it draws.
#[derive(Debug, Clone, Default)]
pub struct Shown {
    pub commands: Vec<UserCommand>,
    pub items: Vec<StartItem>,
    /// The key of the command or the entry the form shows, as the palette prints it.
    pub key: Option<String>,
    /// The own command's symbol at the bars' size, once read (`button_bar::symbols`).
    pub icon: Option<iced::widget::image::Handle>,
    /// The bars' symbol size, in pixels: the preview's.
    pub icon_size: u16,
}

fn wrap(message: UcMsg) -> Message {
    Message::UserCommands(message)
}

/// The name of the key of what the form shows: the stored own command's own name, or a stored
/// Start menu entry's `start:<id>`; `None` for a new one, a submenu or a separator.
pub fn key_name(window: &CommandsWindow) -> Option<String> {
    match window.tab {
        Tab::Own => {
            let id = window.own.as_ref()?.id?;
            known::commands()
                .into_iter()
                .find(|command| command.id == id)
                .map(|command| command.name)
        }
        Tab::Start => window
            .start
            .as_ref()
            .filter(|form| form.kind == StartItemKind::Item)
            .and_then(|form| form.id)
            .map(start_key_name),
    }
}

/// The window over the main window, as a card.
pub fn layer<'a>(
    window: &'a CommandsWindow,
    shown: Shown,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    theme::large_dialog(
        Message::dialog_head(fit::label(t!("user_commands.title")).size(18)),
        body(window, shown, tokens),
        tokens,
    )
}

/// The tabs, the page and the one row of buttons: the card's content over the main window, the
/// whole of its own window.
pub fn body<'a>(
    window: &'a CommandsWindow,
    shown: Shown,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    // "Choose symbol..." takes the page while it is open (RC-110-10).
    if let Some(picker) = &window.picker {
        return symbol_picker::view::page(picker, super::picker_message, tokens);
    }
    // The tabs are one stop of the ring; the arrows choose.
    let tabs = page_tabs::strip(
        &[
            (Tab::Own, t!("user_commands.tab.own")),
            (Tab::Start, t!("user_commands.tab.start")),
        ],
        window.tab,
        "user_commands.tabs",
        |tab| wrap(UcMsg::Tab(tab)),
        tokens,
    );
    let (list, form) = match window.tab {
        Tab::Own => (
            own::list_side(window, &shown.commands, tokens),
            own::form_side(window, &shown, tokens),
        ),
        Tab::Start => (
            start::tree_side(window, &shown.items, tokens),
            start::form_side(window, shown.key.clone(), tokens),
        ),
    };
    let main = row![
        // The list and the form share the width, 2:3.
        container(list)
            .width(Length::FillPortion(2))
            .height(Length::Fill),
        container(form)
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
    let buttons = Row::new()
        .push(container(note).width(Length::Fill))
        .push(layout::primary_icon_button(
            Icon::Save,
            t!("user_commands.save"),
            (window.dirty() && !window.busy).then(|| wrap(UcMsg::Save)),
        ))
        .push(layout::secondary_icon_button(
            Icon::Close,
            t!("user_commands.close"),
            Some(wrap(UcMsg::Close)),
        ))
        .spacing(12)
        .align_y(Alignment::Center);
    column![tabs, main, buttons].spacing(12).into()
}

/// "Delete ...?", Delete and Keep.
fn delete_question<'a>(question: String, tokens: &'static Tokens) -> Element<'a, Message> {
    container(
        column![
            fit::label(question),
            row![
                Space::new().width(Length::Fill),
                command::small(None, t!("user_commands.yes"), Some(UcMsg::Confirm(true))),
                command::small(None, t!("user_commands.no"), Some(UcMsg::Confirm(false))),
            ]
            .spacing(6),
        ]
        .spacing(6),
    )
    .padding(8)
    .style(move |_| theme::panel(tokens, true))
    .into()
}
