//! Drawing the quick connection dialog: a medium card over the dimmed window that takes every
//! click, or a window of its own (RC-080-23, `dialog_windows.rs`).

use iced::{
    Alignment, Element,
    widget::{column, container, row, text_input},
};
use rc_engine::VaultStateKind;
use rc_vault::ExposeSecret as _;

use super::{QuickDialog, QuickMsg, QuickPhase};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
    },
    i18n::t,
    theme::{self, Tokens, fit::prose, focus, icon::Icon, layout},
};

const LABEL_WIDTH: f32 = 140.0;

/// The host field, which takes the keyboard when the dialog's window opens.
pub const HOST_FIELD: &str = "rcommander.quick.host";

/// The dialog over the window.
pub fn overlay<'a>(dialog: &'a QuickDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    Message::overlay(
        t!("quick.title"),
        body(dialog, tokens),
        layout::MEDIUM_DIALOG_WIDTH,
        tokens,
    )
}

/// The dialog's fields and buttons: on its card, or filling a window of its own.
pub fn body<'a>(dialog: &'a QuickDialog, tokens: &'static Tokens) -> Element<'a, Message> {
    let editing = dialog.phase != QuickPhase::Connecting;
    let mut body = column![
        host_field(&dialog.host, editing, tokens),
        field(
            t!("quick.port"),
            &dialog.port,
            false,
            editing,
            QuickMsg::Port,
            tokens
        ),
        field(
            t!("quick.user"),
            &dialog.user,
            false,
            editing,
            QuickMsg::User,
            tokens
        ),
        field(
            t!("quick.password"),
            dialog.password.expose_secret(),
            true,
            editing,
            |typed| QuickMsg::Password(typed.into()),
            tokens
        ),
        row![
            field(
                t!("quick.key_path"),
                &dialog.key_path,
                false,
                editing,
                QuickMsg::KeyPath,
                tokens
            ),
            browse::button(PathField::QuickKey, &dialog.key_path, editing),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
        field(
            t!("quick.passphrase"),
            dialog.passphrase.expose_secret(),
            true,
            editing,
            |typed| QuickMsg::Passphrase(typed.into()),
            tokens
        ),
    ]
    .spacing(8);
    body = body.push(layout::check(
        t!("quick.save"),
        dialog.save,
        editing,
        |on| Message::QuickConnect(QuickMsg::Save(on)),
    ));
    if dialog.save {
        body = body.push(field(
            t!("quick.name"),
            &dialog.name,
            false,
            editing,
            QuickMsg::Name,
            tokens,
        ));
    }
    if let Some(state) = &dialog.vault {
        let (note, label) = match state {
            VaultStateKind::Absent => (t!("quick.vault_create"), t!("quick.master_new")),
            _ => (t!("quick.vault_unlock"), t!("quick.master")),
        };
        body = body.push(prose(note).color(tokens.text_secondary));
        body = body.push(field(
            label,
            dialog.master.expose_secret(),
            true,
            editing,
            |typed| QuickMsg::Master(typed.into()),
            tokens,
        ));
        if *state == VaultStateKind::Absent {
            body = body.push(field(
                t!("connections.vault.repeat"),
                dialog.master_repeat.expose_secret(),
                true,
                editing,
                |typed| QuickMsg::MasterRepeat(typed.into()),
                tokens,
            ));
        }
    }
    match &dialog.phase {
        QuickPhase::Editing => {}
        QuickPhase::Connecting => {
            body = body.push(prose(t!("quick.connecting")).color(tokens.text_secondary));
        }
        QuickPhase::Failed(error) => {
            body = body.push(prose(error.as_str()).color(theme::DANGER));
        }
    }
    let buttons = layout::button_row(
        Some(layout::primary_icon_button(
            Icon::Connect,
            t!("quick.connect"),
            editing.then(|| Message::QuickConnect(QuickMsg::Confirm)),
        )),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("quick.close"),
            Some(Message::QuickConnect(QuickMsg::Close)),
        )],
        [],
    );
    body.push(buttons).into()
}

/// The host's label and field, which carries [`HOST_FIELD`].
fn host_field<'a>(value: &'a str, editing: bool, tokens: &'static Tokens) -> Element<'a, Message> {
    let mut input = text_input("", value)
        .id(HOST_FIELD)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([4, 8]);
    if editing {
        input = input.on_input(|typed| Message::QuickConnect(QuickMsg::Host(typed)));
    }
    labelled(t!("quick.host"), input, tokens)
}

/// A label and its text field; `secure` hides what is typed.
fn field<'a>(
    label: &'static str,
    value: &'a str,
    secure: bool,
    editing: bool,
    message: impl Fn(String) -> QuickMsg + 'a,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let mut input = text_input("", value)
        .secure(secure)
        .font(theme::UI_FONT)
        .size(theme::TEXT_SIZE)
        .padding([4, 8]);
    if editing {
        input = input.on_input(move |typed| Message::QuickConnect(message(typed)));
    }
    labelled(label, input, tokens)
}

/// `label` beside `input`, which answers to the label's access key.
fn labelled<'a>(
    label: &'static str,
    input: text_input::TextInput<'a, Message>,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    row![
        container(layout::caption(label, Some(tokens.text_secondary))).width(LABEL_WIDTH),
        layout::keyed(focus::Stop::field(input, label), label)
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}
