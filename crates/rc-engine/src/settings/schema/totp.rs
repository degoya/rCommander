//! One-time codes at the SSH login (RC-060-08): whether a server's question for a code is
//! answered with the code of the connection's stored seed. On by default; off, the question goes
//! to the person as it does without a seed, and a login script's explicit `send_totp` step still
//! sends one. Protected: whether a stored second factor answers by itself is a person's choice,
//! never an agent's. On the Terminal page, with the logins.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether the code of a stored seed answers the server's question by itself.
pub const TOTP_AUTOFILL_KEY: &str = "connections.totp_autofill";

pub(super) const TOTP_AUTOFILL: SettingSpec = SettingSpec {
    key: TOTP_AUTOFILL_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.connections.totp_autofill",
    help: "setting.connections.totp_autofill.help",
    protected: true,
    restart: false,
};
