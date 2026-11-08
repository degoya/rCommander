//! The controls of the form, in the order Tab walks them, and the field an engine error code is
//! about. Out of `form.rs`, which was full.

use rc_engine::SecretKind;

/// A control of the form, in the order Tab walks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Field {
    Name,
    Kind,
    Host,
    Port,
    User,
    Auth,
    /// The shared credential a connection takes its user and secrets from (RC-050-12).
    Credential,
    Password,
    KeyPath,
    ImportKey,
    Passphrase,
    /// Remove the stored secret of this kind when saving.
    Forget(SecretKind),
    Script,
    View,
    StartDirectory,
    Notes,
    /// The theme of its panel and SSH terminal in the light and in the dark mode (RC-100-51).
    ThemeLight,
    ThemeDark,
    Favourite,
    /// The seed of the one-time codes the SSH login answers with (RC-060-08).
    TotpSeed,
    /// The saved SSH connection it connects through (RC-110-03).
    Jump,
}

impl Field {
    /// The widget id of the field's control: a text field's, or that of the stop of the focus
    /// ring a choice or a switch is (RC-080-16), by which the manager follows the ring.
    pub fn id(self) -> &'static str {
        match self {
            Self::Name => "rcommander.connections.name",
            Self::Host => "rcommander.connections.host",
            Self::Port => "rcommander.connections.port",
            Self::User => "rcommander.connections.user",
            Self::Password => "rcommander.connections.password",
            Self::KeyPath => "rcommander.connections.key_path",
            Self::Passphrase => "rcommander.connections.passphrase",
            Self::StartDirectory => "rcommander.connections.start_directory",
            Self::Notes => "rcommander.connections.notes",
            Self::Kind => "rcommander.connections.kind",
            Self::Auth => "rcommander.connections.auth",
            Self::Credential => "rcommander.connections.credential",
            Self::ImportKey => "rcommander.connections.import_key",
            Self::Forget(SecretKind::Password) => "rcommander.connections.forget_password",
            Self::Forget(SecretKind::Key) => "rcommander.connections.forget_key",
            Self::Forget(SecretKind::Passphrase) => "rcommander.connections.forget_passphrase",
            Self::Forget(SecretKind::TotpSeed) => "rcommander.connections.forget_totp_seed",
            Self::Script => "rcommander.connections.script",
            Self::View => "rcommander.connections.view",
            Self::ThemeLight => "rcommander.connections.theme_light",
            Self::ThemeDark => "rcommander.connections.theme_dark",
            Self::Favourite => "rcommander.connections.favourite",
            Self::TotpSeed => "rcommander.connections.totp_seed",
            Self::Jump => "rcommander.connections.jump",
        }
    }

    /// The field an engine error code is about; `None` for one about the whole save.
    pub fn of_code(code: &str) -> Option<Self> {
        Some(match code {
            "name_empty" | "name_taken" => Self::Name,
            "host_empty" | "host_invalid" => Self::Host,
            "port_invalid" => Self::Port,
            "username_empty" => Self::User,
            "auth_unsupported" => Self::Auth,
            "credential_not_found" => Self::Credential,
            "view_unsupported" => Self::View,
            // The engine names no mode: the light one's list stands first (RC-100-51).
            "theme_unknown" | "theme_wrong_mode" => Self::ThemeLight,
            "secret_empty" => Self::Password,
            "key_format_unknown" | "key_file_unreadable" | "key_path_refused" => Self::KeyPath,
            "key_passphrase_needed" | "key_passphrase_wrong" => Self::Passphrase,
            "totp_seed_invalid" => Self::TotpSeed,
            "jump_cycle" | "jump_chain_too_long" | "jump_needs_ssh" => Self::Jump,
            _ => return None,
        })
    }
}

/// An id no widget has: focusing it takes the keys from every text field.
pub const NO_FIELD: &str = "rcommander.connections.none";
