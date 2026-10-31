//! The right side: the form of the connection at the cursor (or a new one), each field with its
//! error under it, or the master-password prompt while it is open. A secret field shows only
//! what is typed now - never a stored value - and only in clear while the vault is not locked.

use std::fmt;

use iced::{
    Alignment, Element, Length,
    widget::{Column, Space, checkbox, column, container, pick_list, row, scrollable},
};
use rc_engine::{
    AuthMethod, ConnectionId, ConnectionKind, DefaultView, SecretKind, SshState, VaultStateKind,
};
use rc_vault::{ExposeSecret as _, SecretString};

pub(super) use super::vault_prompt::{vault_fields, vault_prompt};

use super::{Actions, field, hint, label, wrap};
use crate::{
    app::{
        Message,
        browse::{self, PathField},
        connections::{
            ConnMsg, ConnectionManager, Focus,
            form::{Edit, Field, Form, theme_choices},
        },
    },
    i18n::t,
    theme::{self, Tokens, focus, icon::Icon, layout},
};

/// The width of the labels in front of the controls.
const LABEL_WIDTH: f32 = 150.0;

pub(super) fn side<'a>(
    manager: &'a ConnectionManager,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    if let Some(prompt) = &manager.prompt {
        return vault_prompt(prompt, tokens);
    }
    let Some(form) = &manager.form else {
        return container(hint(t!("connections.choose"), tokens))
            .padding(8)
            .into();
    };
    let current = match manager.focus {
        Focus::Field(field) => Some(field),
        _ => None,
    };
    let locked = manager.vault == Some(VaultStateKind::Locked);
    let mut rows = Column::new().spacing(8);
    for shown in form.fields() {
        rows = rows.push(field_row(
            manager,
            form,
            shown,
            current == Some(shown),
            locked,
            tokens,
        ));
    }
    if form.uses_password() || form.uses_key() {
        rows = rows.push(layout::check(
            t!("connections.reveal"),
            form.reveal,
            !locked,
            |on| wrap(ConnMsg::Reveal(on)),
        ));
    }
    // Where a stored connection's secrets come from (RC-100-34); not from a shared credential.
    if form.id().is_some()
        && form.shared.is_none()
        && let Some(panel) = super::sources::panel(manager, form, tokens)
    {
        rows = rows.push(panel);
    }
    // A session: of the connection as stored, so not while the form has edits.
    let stored = form.id().filter(|_| !form.dirty);
    let saved = form.id().is_some() || form.credential_id().is_some();
    let title = match (form.is_credential(), saved) {
        (true, true) => t!("connections.credential_title"),
        (true, false) => t!("connections.new_credential_title"),
        (false, true) => t!("connections.edit_title"),
        (false, false) => t!("connections.new_title"),
    };
    let mut side = column![
        label(title).font(theme::UI_FONT_STRONG),
        scrollable(rows.padding([0, 12])).height(Length::Fill),
    ]
    .spacing(10);
    if form.kind == ConnectionKind::Ssh
        && let Some(id) = stored
    {
        side = side.push(session_line(manager, id, tokens));
        side = side.push(super::remote::ssh_actions(id));
    }
    if matches!(
        form.kind,
        ConnectionKind::Ftps | ConnectionKind::FtpsImplicit
    ) && let Some(id) = stored
    {
        side = side.push(super::remote::certificates(manager, id, tokens));
    }
    side.into()
}

/// The form's Test (an extra action, at the left) and Save (the primary, beside Close), for the
/// card's row of buttons; none while no form shows or the master-password prompt stands in its
/// place.
pub(super) fn actions<'a>(manager: &ConnectionManager) -> Actions<'a> {
    let Some(form) = manager.form.as_ref().filter(|_| manager.prompt.is_none()) else {
        return Actions::default();
    };
    // A test: of the connection as stored, so not while the form has edits.
    let stored = form.id().filter(|_| !form.dirty);
    Actions {
        left: vec![layout::secondary_icon_button(
            Icon::Test,
            t!("connections.test"),
            stored.map(|id| wrap(ConnMsg::Test(id))),
        )],
        right: vec![layout::primary_icon_button(
            Icon::Save,
            t!("connections.save"),
            (!manager.busy).then(|| wrap(ConnMsg::Save)),
        )],
    }
}

/// The SSH session of the stored connection `id`: its state, and Connect or Disconnect.
fn session_line<'a>(
    manager: &'a ConnectionManager,
    id: ConnectionId,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let (state, action) = match manager.session(id) {
        None => (
            t!("connections.session.none").to_owned(),
            (
                Icon::Connect,
                t!("connections.connect"),
                ConnMsg::Connect(id),
            ),
        ),
        Some(status) if status.state == SshState::Connecting => (
            t!("connections.session.connecting").to_owned(),
            (
                Icon::Disconnect,
                t!("connections.disconnect"),
                ConnMsg::Disconnect(id),
            ),
        ),
        Some(status) => (
            t!("connections.session.connected", channels = status.channels),
            (
                Icon::Disconnect,
                t!("connections.disconnect"),
                ConnMsg::Disconnect(id),
            ),
        ),
    };
    row![
        hint(state, tokens).width(Length::Fill),
        layout::secondary_icon_button(action.0, action.1, Some(wrap(action.2))),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

/// One field: its label (marked while the keyboard is on it), its control, its error.
fn field_row<'a>(
    manager: &'a ConnectionManager,
    form: &'a Form,
    shown: Field,
    current: bool,
    locked: bool,
    tokens: &'static Tokens,
) -> Element<'a, Message> {
    let control: Element<'a, Message> = match shown {
        Field::Name => text(&form.name, shown, Edit::Name),
        Field::Host => text(&form.host, shown, Edit::Host),
        Field::Port => text(&form.port, shown, Edit::Port),
        Field::User => text(&form.user, shown, Edit::User),
        Field::KeyPath => row![
            text(&form.key_path, shown, Edit::KeyPath),
            browse::button(PathField::ConnectionKey, &form.key_path, true),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
        .into(),
        Field::StartDirectory => text(&form.start_directory, shown, Edit::StartDirectory),
        Field::Notes => text(&form.notes, shown, Edit::Notes),
        Field::Password => secret(
            form,
            SecretKind::Password,
            &form.password,
            form.stored.has_password,
        ),
        Field::Passphrase => secret(
            form,
            SecretKind::Passphrase,
            &form.passphrase,
            form.stored.has_passphrase,
        ),
        Field::Credential => {
            let own = Labeled(None, t!("connections.credential_own").to_owned());
            let shared = manager
                .tree
                .credentials()
                .into_iter()
                .map(|entry| Labeled(Some(entry.id), entry.name.clone()));
            choice(
                std::iter::once(own).chain(shared),
                form.shared,
                ConnMsg::Shared,
                shown,
            )
        }
        Field::Kind => choice(
            ConnectionKind::ALL
                .iter()
                .map(|kind| Labeled(*kind, kind_label(*kind).to_owned())),
            form.kind,
            ConnMsg::Kind,
            shown,
        ),
        Field::Auth => choice(
            AuthMethod::ALL
                .iter()
                .map(|auth| Labeled(*auth, auth_label(*auth).to_owned())),
            form.auth,
            ConnMsg::Auth,
            shown,
        ),
        Field::View => choice(
            DefaultView::ALL
                .iter()
                .map(|view| Labeled(*view, view_label(*view).to_owned())),
            form.view,
            ConnMsg::View,
            shown,
        ),
        Field::Script => {
            let none = Labeled(None, t!("connections.script_none").to_owned());
            let scripts = manager
                .scripts
                .iter()
                .map(|(id, name)| Labeled(Some(*id), name.clone()));
            choice(
                std::iter::once(none).chain(scripts),
                form.script,
                ConnMsg::Script,
                shown,
            )
        }
        Field::ThemeLight => theme_choice(form, false, ConnMsg::ThemeLight, shown),
        Field::ThemeDark => theme_choice(form, true, ConnMsg::ThemeDark, shown),
        Field::ImportKey => switch(form.import_key, t!("connections.import_key"), shown),
        Field::Favourite => switch(form.favourite, t!("connections.favourite"), shown),
        Field::Forget(kind) => switch(form.forget.contains(&kind), forget_label(kind), shown),
    };
    let marker = if current { "\u{25B8} " } else { "" };
    let mut column = column![
        row![
            label(format!("{marker}{}", field_label(shown))).width(LABEL_WIDTH),
            container(control).width(Length::Fill),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    ]
    .spacing(2);
    if shown == Field::Password || shown == Field::Passphrase {
        let note = if locked {
            t!("connections.secret_locked")
        } else {
            t!("connections.secret_note")
        };
        column = column.push(indented(hint(note, tokens).into()));
    }
    if shown == Field::User
        && let Some(entry) = form.shared.and_then(|id| manager.tree.credential(id))
    {
        let user = entry.username.as_deref().unwrap_or_default();
        let note = t!("connections.credential_user", user = user);
        column = column.push(indented(hint(note, tokens).into()));
    }
    if shown == Field::KeyPath
        && let Some(key) = &form.stored_key
    {
        let stored = t!(
            "connections.key_stored",
            key_type = key.key_type,
            fingerprint = key.fingerprint
        );
        column = column.push(indented(hint(stored, tokens).into()));
    }
    if let Some(error) = form.error(shown) {
        column = column.push(indented(label(error).size(12).color(theme::DANGER).into()));
    }
    column.into()
}

fn indented(content: Element<'_, Message>) -> Element<'_, Message> {
    row![Space::new().width(LABEL_WIDTH + 8.0), content].into()
}

fn text<'a>(value: &str, shown: Field, edit: fn(String) -> Edit) -> Element<'a, Message> {
    field("", value)
        .id(shown.id())
        .on_input(move |typed| wrap(ConnMsg::Edit(edit(typed))))
        .on_submit(wrap(ConnMsg::Save))
        .into()
}

/// A secret field: what is typed now, hidden unless shown; its placeholder says whether one
/// is stored.
fn secret<'a>(
    form: &Form,
    kind: SecretKind,
    typed: &SecretString,
    stored: bool,
) -> Element<'a, Message> {
    let placeholder = if stored {
        t!("connections.secret_stored")
    } else {
        t!("connections.secret_none")
    };
    let id = if kind == SecretKind::Passphrase {
        Field::Passphrase.id()
    } else {
        Field::Password.id()
    };
    field(placeholder, typed.expose_secret())
        .id(id)
        .secure(!form.reveal)
        .on_input(move |value| wrap(ConnMsg::Secret(kind, SecretString::from(value))))
        .on_submit(wrap(ConnMsg::Save))
        .into()
}

/// A value and its text in a list.
#[derive(Debug, Clone, PartialEq)]
struct Labeled<T>(T, String);

impl<T> fmt::Display for Labeled<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.1)
    }
}

/// The theme of the light or the `dark` mode (RC-100-51): Default, then each theme of the mode;
/// one the window no longer knows shows as Default, which saving then stores.
fn theme_choice<'a>(
    form: &Form,
    dark: bool,
    message: fn(Option<String>) -> ConnMsg,
    shown: Field,
) -> Element<'a, Message> {
    let themes = theme_choices(dark);
    let current = form
        .theme(dark)
        .filter(|id| themes.iter().any(|(known, _)| known.as_str() == *id))
        .map(str::to_owned);
    let default = Labeled(None, t!("connections.theme_default").to_owned());
    let options = themes.into_iter().map(|(id, name)| Labeled(Some(id), name));
    choice(
        std::iter::once(default).chain(options),
        current,
        message,
        shown,
    )
}

/// A choice of the form: a stop of the focus ring by the field's id; the arrows choose the option
/// before and after, as a closed combo box does (RC-080-53), and Space changes it through the
/// manager's keys (`keys.rs`).
fn choice<'a, T: Clone + PartialEq + 'static>(
    options: impl Iterator<Item = Labeled<T>>,
    current: T,
    message: fn(T) -> ConnMsg,
    shown: Field,
) -> Element<'a, Message> {
    let options: Vec<Labeled<T>> = options.collect();
    let selected = options.iter().find(|option| option.0 == current).cloned();
    let pick = move |picked: Labeled<T>| wrap(message(picked.0));
    let moves = focus::moves(&options, selected.as_ref(), &pick);
    let list = pick_list(options, selected, pick)
        .text_size(theme::TEXT_SIZE)
        .width(260);
    focus::Stop::pick(list, shown.id(), moves)
        .id(shown.id())
        .into()
}

/// A switch of the form: a stop of the focus ring by the field's id; Space toggles it.
fn switch<'a>(on: bool, title: &'static str, shown: Field) -> Element<'a, Message> {
    let check = checkbox(on)
        .label(title)
        .size(14)
        .text_size(theme::TEXT_SIZE)
        .on_toggle(move |_| wrap(ConnMsg::Toggle(shown)));
    focus::Stop::toggle(check, shown.id(), Some(wrap(ConnMsg::Toggle(shown))))
        .id(shown.id())
        .into()
}

fn field_label(shown: Field) -> &'static str {
    match shown {
        Field::Name => t!("connections.field.name"),
        Field::Kind => t!("connections.field.kind"),
        Field::Host => t!("connections.field.host"),
        Field::Port => t!("connections.field.port"),
        Field::User => t!("connections.field.user"),
        Field::Auth => t!("connections.field.auth"),
        Field::Credential => t!("connections.field.credential"),
        Field::Password => t!("connections.field.password"),
        Field::KeyPath => t!("connections.field.key_path"),
        Field::ImportKey | Field::Favourite | Field::Forget(_) => "",
        Field::Passphrase => t!("connections.field.passphrase"),
        Field::Script => t!("connections.field.script"),
        Field::View => t!("connections.field.view"),
        Field::StartDirectory => t!("connections.field.start_directory"),
        Field::Notes => t!("connections.field.notes"),
        Field::ThemeLight => t!("connections.field.theme_light"),
        Field::ThemeDark => t!("connections.field.theme_dark"),
    }
}

fn forget_label(kind: SecretKind) -> &'static str {
    match kind {
        SecretKind::Password => t!("connections.forget.password"),
        SecretKind::Key => t!("connections.forget.key"),
        SecretKind::Passphrase => t!("connections.forget.passphrase"),
    }
}

/// A protocol's name; the FTP kinds of RC-070-01 by their stored name.
pub fn kind_label(kind: ConnectionKind) -> &'static str {
    match kind.as_str() {
        "ssh" => t!("connections.kind.ssh"),
        "ftp" => t!("connections.kind.ftp"),
        "ftps" => t!("connections.kind.ftps"),
        "ftps_implicit" => t!("connections.kind.ftps_implicit"),
        other => other,
    }
}

fn auth_label(auth: AuthMethod) -> &'static str {
    match auth.as_str() {
        "password" => t!("connections.auth.password"),
        "key" => t!("connections.auth.key"),
        "agent" => t!("connections.auth.agent"),
        "interactive" => t!("connections.auth.interactive"),
        "anonymous" => t!("connections.auth.anonymous"),
        other => other,
    }
}

fn view_label(view: DefaultView) -> &'static str {
    match view.as_str() {
        "files" => t!("connections.view.files"),
        "terminal" => t!("connections.view.terminal"),
        other => other,
    }
}
