//! Drawing the manager on the window's dialog layer: the tree on the left (`view/tree.rs`), the
//! form or the master-password prompt on the right (`view/form.rs`), the note and the buttons
//! below in one row; an import page in place of both (`view/import.rs`, `view/tc_import.rs`,
//! `view/rdm_import.rs`), its options and buttons in the same row, the Total Commander one under its own title. The
//! manager is a large dialog: its card fills the window but for a margin, so a form scrolls only
//! in a window too small for it.

mod form;
mod import;
mod rdm_import;
mod remote;
mod sources;
mod tc_import;
mod tree;
mod vault_prompt;

use iced::{
    Alignment, Element, Length,
    widget::{Row, Space, column, container, row, text, text_input},
};

use super::{
    ConnMsg, ConnectionManager,
    vault::{AfterVault, Ask, VaultPrompt},
};
use crate::{
    app::Message,
    i18n::t,
    theme::{self, Tokens, icon::Icon, layout},
};

pub(super) use crate::theme::fit::{cell, cut, hint, label, path_cell, table_row};

/// The name field of a new or renamed node in the tree.
pub const NAME_FIELD: &str = "rcommander.connections.tree_name";
/// The tree's filter.
pub const FILTER_FIELD: &str = "rcommander.connections.filter";
/// The tree's scrolled list, kept on the cursor by the keyboard.
pub const TREE_SCROLL_ID: &str = "rcommander.connections.tree";
/// The tree as a stop of the focus ring (RC-080-16).
pub const TREE_STOP_ID: &str = "rcommander.connections.tree_stop";
/// The master-password field.
pub const VAULT_FIELD: &str = "rcommander.connections.vault";

fn wrap(message: ConnMsg) -> Message {
    Message::Connections(message)
}

/// A page's share of the card's one row of buttons: options on the left, buttons beside Close.
#[derive(Default)]
struct Actions<'a> {
    left: Vec<Element<'a, Message>>,
    right: Vec<Element<'a, Message>>,
}

/// The manager over the window: a large dialog, or the small unlock prompt a connection asked
/// for on its own.
pub fn layer<'a>(manager: &'a ConnectionManager, tokens: &'static Tokens) -> Element<'a, Message> {
    if manager.prompt_only
        && let Some(prompt) = &manager.prompt
    {
        let head = Message::dialog_head(label(prompt_words(prompt).0).font(theme::UI_FONT_STRONG));
        return layout::dialog_card(
            head,
            form::vault_fields(prompt, tokens),
            layout::SMALL_DIALOG_WIDTH,
            tokens,
        );
    }
    theme::large_dialog(
        Message::dialog_head(label(title(manager)).size(18)),
        body(manager, tokens),
        tokens,
    )
}

/// The unlock prompt a connection asked for alone, without its head: the content of its window
/// of its own (RC-080-23, `dialog_windows/questions.rs`); `None` while the manager shows more.
pub fn prompt_body<'a>(
    manager: &'a ConnectionManager,
    tokens: &'static Tokens,
) -> Option<Element<'a, Message>> {
    let prompt = manager.prompt.as_ref().filter(|_| manager.prompt_only)?;
    Some(form::vault_fields(prompt, tokens))
}

/// The title of the prompt a connection asked for alone, for its window's title bar.
pub fn prompt_title(manager: &ConnectionManager) -> Option<&'static str> {
    let prompt = manager.prompt.as_ref().filter(|_| manager.prompt_only)?;
    Some(prompt_words(prompt).0)
}

/// The manager's title: the settings open the Total Commander import, which takes more than
/// connections.
pub fn title(manager: &ConnectionManager) -> &'static str {
    if manager.tc_import.is_some() {
        t!("tc_import.title")
    } else {
        t!("connections.title")
    }
}

/// The manager's page and its one row of buttons: the card's content over the main window, the
/// whole of its own window (RC-080-15).
pub fn body<'a>(manager: &'a ConnectionManager, tokens: &'static Tokens) -> Element<'a, Message> {
    let (body, actions): (Element<'a, Message>, Actions<'a>) =
        match (&manager.tc_import, &manager.import, &manager.rdm_import) {
            (Some(dialog), _, _) => match &manager.prompt {
                Some(prompt) => (form::vault_prompt(prompt, tokens), Actions::default()),
                None => (tc_import::page(dialog, tokens), tc_import::actions(dialog)),
            },
            (None, Some(dialog), _) => match &manager.prompt {
                Some(prompt) => (form::vault_prompt(prompt, tokens), Actions::default()),
                None => (
                    import::page(manager, dialog, tokens),
                    import::actions(dialog),
                ),
            },
            (None, None, Some(dialog)) => (
                rdm_import::page(dialog, tokens),
                rdm_import::actions(dialog),
            ),
            (None, None, None) => (
                row![
                    // The tree and the form share the window's width, 2:3.
                    container(tree::side(manager, tokens))
                        .width(Length::FillPortion(2))
                        .height(Length::Fill),
                    container(form::side(manager, tokens))
                        .width(Length::FillPortion(3))
                        .height(Length::Fill),
                ]
                .spacing(16)
                .height(Length::Fill)
                .into(),
                form::actions(manager),
            ),
        };
    let note: Element<'a, Message> = match &manager.note {
        Some((note, error)) => {
            let color = if *error {
                theme::DANGER
            } else {
                tokens.text_secondary
            };
            label(note.as_str())
                .color(color)
                .wrapping(text::Wrapping::WordOrGlyph)
                .into()
        }
        None => Space::new().into(),
    };
    // One row: the page's options, the note, the page's buttons and Close.
    let buttons = Row::with_children(actions.left)
        .push(container(note).width(Length::Fill))
        .extend(actions.right)
        .push(layout::secondary_icon_button(
            Icon::Close,
            t!("connections.close"),
            Some(wrap(ConnMsg::Close)),
        ))
        .spacing(12)
        .align_y(Alignment::Center);
    column![body, buttons].spacing(12).into()
}

/// The prompt's title and explanation.
pub(super) fn prompt_words(prompt: &VaultPrompt) -> (&'static str, &'static str) {
    match (prompt.ask, &prompt.after) {
        (Ask::Unlock, AfterVault::Nothing | AfterVault::Retry(_)) => (
            t!("connections.vault.unlock_title"),
            t!("connections.vault.needed_help"),
        ),
        (Ask::Create, _) => (
            t!("connections.vault.create_title"),
            t!("connections.vault.create_help"),
        ),
        (Ask::Unlock, _) => (
            t!("connections.vault.unlock_title"),
            t!("connections.vault.unlock_help"),
        ),
    }
}

/// A one-line text field of the dialog's look.
pub(super) fn field<'a>(placeholder: &str, value: &str) -> text_input::TextInput<'a, Message> {
    text_input(placeholder, value)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([5, 8])
}
