//! The plugins' settings (RC-100-10, RC-100-09; `docs/plugins.md` "Settings"), on the Plugins
//! page. Every `plugins.*` and `plugin.*` key is protected like `mcp.*` (`lookup::is_protected`):
//! they decide which foreign code runs and how its output is shown, so only a person in the
//! window changes them. A plugin's own settings, `plugin.<id>.<key>`, are no rows here: its
//! manifest declares them, and the engine checks them against it (`plugin_admin`).
//! `plugins.automatic_updates` comes with the store (decision 10).

use super::{SettingKind, SettingPage, SettingSpec};

/// The master switch: off, no plugin runs.
pub const ENABLED_KEY: &str = "plugins.enabled";
/// Failures within five minutes before a plugin is suspended until the person resumes it.
pub const CRASH_LIMIT_KEY: &str = "plugins.crash_limit";
/// The most a viewer's text output may write, in MiB.
pub const OUTPUT_LIMIT_KEY: &str = "plugins.viewer_output_limit_mib";
/// Whether a viewer plugin's coloured text (`highlighted`) shows its colours; off shows it as
/// plain text.
pub const VIEWER_TOKEN_COLORS_KEY: &str = "plugins.viewer_token_colors";
/// Which plugin shows an extension, ahead of what the manifests claim: `csv=<plugin id> ...`.
pub const ASSOCIATIONS_KEY: &str = "plugins.associations";
/// Unsigned packages are accepted, marked as such everywhere.
pub const DEVELOPMENT_MODE_KEY: &str = "plugins.development_mode";

pub(super) const ENABLED: SettingSpec = SettingSpec {
    key: ENABLED_KEY,
    page: SettingPage::Plugins,
    kind: SettingKind::Bool { default: true },
    label: "setting.plugins.enabled",
    help: "setting.plugins.enabled.help",
    protected: true,
    restart: false,
};

pub(super) const CRASH_LIMIT: SettingSpec = SettingSpec {
    key: CRASH_LIMIT_KEY,
    page: SettingPage::Plugins,
    kind: SettingKind::Int {
        min: rc_plugin_host::MIN_CRASH_LIMIT as i64,
        max: rc_plugin_host::MAX_CRASH_LIMIT as i64,
        default: rc_plugin_host::DEFAULT_CRASH_LIMIT as i64,
    },
    label: "setting.plugins.crash_limit",
    help: "setting.plugins.crash_limit.help",
    protected: true,
    restart: false,
};

pub(super) const OUTPUT_LIMIT: SettingSpec = SettingSpec {
    key: OUTPUT_LIMIT_KEY,
    page: SettingPage::Plugins,
    kind: SettingKind::Int {
        min: 1,
        max: 4096,
        default: 256,
    },
    label: "setting.plugins.viewer_output_limit_mib",
    help: "setting.plugins.viewer_output_limit_mib.help",
    protected: true,
    restart: false,
};

pub(super) const VIEWER_TOKEN_COLORS: SettingSpec = SettingSpec {
    key: VIEWER_TOKEN_COLORS_KEY,
    page: SettingPage::Plugins,
    kind: SettingKind::Bool { default: true },
    label: "setting.plugins.viewer_token_colors",
    help: "setting.plugins.viewer_token_colors.help",
    protected: true,
    restart: false,
};

pub(super) const ASSOCIATIONS: SettingSpec = SettingSpec {
    key: ASSOCIATIONS_KEY,
    page: SettingPage::Plugins,
    kind: SettingKind::Associations { default: "" },
    label: "setting.plugins.associations",
    help: "setting.plugins.associations.help",
    protected: true,
    restart: false,
};

pub(super) const DEVELOPMENT_MODE: SettingSpec = SettingSpec {
    key: DEVELOPMENT_MODE_KEY,
    page: SettingPage::Plugins,
    kind: SettingKind::Bool { default: false },
    label: "setting.plugins.development_mode",
    help: "setting.plugins.development_mode.help",
    protected: true,
    restart: false,
};
