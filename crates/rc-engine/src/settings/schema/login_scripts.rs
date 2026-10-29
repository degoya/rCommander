//! The login-script editor's warning (RC-050-13): a "Send secret" step whose step before does not
//! wait for a password prompt is marked - the secret would be typed into whatever waits there, a
//! shell among them, and could end up in its history. On by default; not protected: it only adds
//! a line to the editor. On the Terminal page, where the scripts run.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether the editor warns at a secret typed without a password prompt.
pub const WARN_SECRET_PROMPT_KEY: &str = "login_scripts.warn_secret_prompt";

pub(super) const WARN_SECRET_PROMPT: SettingSpec = SettingSpec {
    key: WARN_SECRET_PROMPT_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.login_scripts.warn_secret_prompt",
    help: "setting.login_scripts.warn_secret_prompt.help",
    protected: false,
    restart: false,
};
