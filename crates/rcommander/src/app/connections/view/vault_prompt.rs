//! The master-password prompt: in place of the form in the manager, or alone in the small
//! dialog a connection asked for.

use iced::{Element, widget::column};
use rc_vault::{ExposeSecret as _, SecretString};

use super::{VAULT_FIELD, field, hint, label, prompt_words, wrap};
use crate::{
    app::{
        Message,
        connections::{
            ConnMsg,
            vault::{Ask, VaultPrompt},
        },
    },
    i18n::t,
    theme::{self, Tokens, fit, icon::Icon, layout},
};

/// The master-password prompt in place of the form.
pub(super) fn vault_prompt<'a>(
    prompt: &'a VaultPrompt,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (title, _) = prompt_words(prompt);
    column![
        label(title).font(theme::UI_FONT_STRONG),
        vault_fields(prompt, tokens)
    ]
    .spacing(10)
    .into()
}

/// [`vault_prompt`] without its title: the small prompt's card carries it in its head.
pub(super) fn vault_fields<'a>(
    prompt: &'a VaultPrompt,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (_, help) = prompt_words(prompt);
    let mut body = column![
        hint(help, tokens),
        field(
            t!("connections.vault.password"),
            prompt.password.expose_secret()
        )
        .id(VAULT_FIELD)
        .secure(true)
        .on_input(|value| wrap(ConnMsg::VaultInput(false, SecretString::from(value))))
        .on_submit(wrap(ConnMsg::VaultSubmit)),
    ]
    .spacing(10)
    .max_width(460);
    if prompt.ask == Ask::Create {
        body = body.push(
            field(
                t!("connections.vault.repeat"),
                prompt.repeat.expose_secret(),
            )
            .secure(true)
            .on_input(|value| wrap(ConnMsg::VaultInput(true, SecretString::from(value))))
            .on_submit(wrap(ConnMsg::VaultSubmit)),
        );
    }
    if let Some(error) = &prompt.error {
        body = body.push(fit::prose(error.as_str()).color(theme::DANGER));
    }
    let buttons = layout::button_row(
        Some(layout::primary_icon_button(
            Icon::Confirm,
            t!("connections.vault.ok"),
            (!prompt.busy).then(|| wrap(ConnMsg::VaultSubmit)),
        )),
        [layout::secondary_icon_button(
            Icon::Close,
            t!("connections.vault.cancel"),
            Some(wrap(ConnMsg::VaultCancel)),
        )],
        [],
    );
    // The card around it is the manager's (a large dialog) or the prompt's own (a small one).
    body.push(buttons).into()
}
