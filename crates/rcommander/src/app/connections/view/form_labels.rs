//! The texts of the form: the labels of its fields and the names of its choices. Out of
//! `form.rs`, which was full.

use rc_engine::{AuthMethod, ConnectionKind, DefaultView, SecretKind};

use crate::{app::connections::form::Field, i18n::t};

pub(super) fn field_label(shown: Field) -> &'static str {
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
        Field::TotpSeed => t!("connections.field.totp_seed"),
        Field::Jump => t!("connections.field.jump"),
    }
}

pub(super) fn forget_label(kind: SecretKind) -> &'static str {
    match kind {
        SecretKind::Password => t!("connections.forget.password"),
        SecretKind::Key => t!("connections.forget.key"),
        SecretKind::Passphrase => t!("connections.forget.passphrase"),
        SecretKind::TotpSeed => t!("connections.forget.totp_seed"),
    }
}

/// A protocol's name; the FTP kinds of RC-070-01 by their stored name.
pub(in crate::app::connections) fn kind_label(kind: ConnectionKind) -> &'static str {
    match kind.as_str() {
        "ssh" => t!("connections.kind.ssh"),
        "ftp" => t!("connections.kind.ftp"),
        "ftps" => t!("connections.kind.ftps"),
        "ftps_implicit" => t!("connections.kind.ftps_implicit"),
        other => other,
    }
}

pub(super) fn auth_label(auth: AuthMethod) -> &'static str {
    match auth.as_str() {
        "password" => t!("connections.auth.password"),
        "key" => t!("connections.auth.key"),
        "agent" => t!("connections.auth.agent"),
        "interactive" => t!("connections.auth.interactive"),
        "anonymous" => t!("connections.auth.anonymous"),
        other => other,
    }
}

pub(super) fn view_label(view: DefaultView) -> &'static str {
    match view.as_str() {
        "files" => t!("connections.view.files"),
        "terminal" => t!("connections.view.terminal"),
        other => other,
    }
}
