//! The self-update's rows (RC-100-05 ... RC-100-07), on the page "Updates". Every `update.*` key
//! is protected (`lookup.rs`): channel, source and automation decide which program runs next,
//! and an agent may loosen that as little as its own rights. The state of the checks is the one
//! setting `update.state`, no row (`rc_update::STATE_KEY`). Invalid stored values read as the
//! safe default (`settings.rs`, `effective`).

use super::{Choice, SettingKind, SettingPage, SettingSpec};

/// Whether rCommander checks by itself; "Check now" always works.
pub const UPDATE_CHECK_KEY: &str = "update.check_enabled";
/// Hours between two checks.
pub const UPDATE_INTERVAL_KEY: &str = "update.check_interval_hours";
/// `stable` or `beta`; until a person chooses, the running version's (a pre-release reads beta).
pub const UPDATE_CHANNEL_KEY: &str = "update.channel";
/// A mirror's https folder; empty for the official GitHub releases.
pub const UPDATE_SOURCE_KEY: &str = "update.source";
/// Empty for the system's proxy, `direct`, or `http(s)://host:port` without a login.
pub const UPDATE_PROXY_KEY: &str = "update.proxy";
/// Whether the first start after an update shows its notes once.
pub const UPDATE_WHATS_NEW_KEY: &str = "update.whats_new_after_update";
/// `off`, `on_quit` or `when_idle` (RC-100-07).
pub const UPDATE_AUTO_KEY: &str = "update.auto_install";
/// Minutes without input before `when_idle` restarts into the update.
pub const UPDATE_IDLE_KEY: &str = "update.idle_minutes";
/// Days the newest database copy stays after a proven update; 0 keeps it for good.
pub const UPDATE_RETENTION_KEY: &str = "update.backup_retention_days";

const CHANNELS: &[Choice] = &[
    Choice {
        value: "stable",
        label: "setting.update.channel.stable",
    },
    Choice {
        value: "beta",
        label: "setting.update.channel.beta",
    },
];

const AUTO: &[Choice] = &[
    Choice {
        value: "off",
        label: "setting.update.auto_install.off",
    },
    Choice {
        value: "on_quit",
        label: "setting.update.auto_install.on_quit",
    },
    Choice {
        value: "when_idle",
        label: "setting.update.auto_install.when_idle",
    },
];

pub(super) const CHECK_ENABLED: SettingSpec = SettingSpec {
    key: UPDATE_CHECK_KEY,
    page: SettingPage::Updates,
    kind: SettingKind::Bool { default: true },
    label: "setting.update.check_enabled",
    help: "setting.update.check_enabled.help",
    protected: true,
    restart: false,
};

pub(super) const CHECK_INTERVAL: SettingSpec = SettingSpec {
    key: UPDATE_INTERVAL_KEY,
    page: SettingPage::Updates,
    kind: SettingKind::Int {
        min: 1,
        max: 168,
        default: 24,
    },
    label: "setting.update.check_interval_hours",
    help: "setting.update.check_interval_hours.help",
    protected: true,
    restart: false,
};

pub(super) const CHANNEL: SettingSpec = SettingSpec {
    key: UPDATE_CHANNEL_KEY,
    page: SettingPage::Updates,
    kind: SettingKind::Choice {
        choices: CHANNELS,
        default: "stable",
    },
    label: "setting.update.channel",
    help: "setting.update.channel.help",
    protected: true,
    restart: false,
};

pub(super) const SOURCE: SettingSpec = SettingSpec {
    key: UPDATE_SOURCE_KEY,
    page: SettingPage::Updates,
    kind: SettingKind::Source { default: "" },
    label: "setting.update.source",
    help: "setting.update.source.help",
    protected: true,
    restart: false,
};

pub(super) const PROXY: SettingSpec = SettingSpec {
    key: UPDATE_PROXY_KEY,
    page: SettingPage::Updates,
    kind: SettingKind::Proxy { default: "" },
    label: "setting.update.proxy",
    help: "setting.update.proxy.help",
    protected: true,
    restart: false,
};

pub(super) const WHATS_NEW: SettingSpec = SettingSpec {
    key: UPDATE_WHATS_NEW_KEY,
    page: SettingPage::Updates,
    kind: SettingKind::Bool { default: true },
    label: "setting.update.whats_new_after_update",
    help: "setting.update.whats_new_after_update.help",
    protected: true,
    restart: false,
};

pub(super) const AUTO_INSTALL: SettingSpec = SettingSpec {
    key: UPDATE_AUTO_KEY,
    page: SettingPage::Updates,
    kind: SettingKind::Choice {
        choices: AUTO,
        default: "off",
    },
    label: "setting.update.auto_install",
    help: "setting.update.auto_install.help",
    protected: true,
    restart: false,
};

pub(super) const IDLE_MINUTES: SettingSpec = SettingSpec {
    key: UPDATE_IDLE_KEY,
    page: SettingPage::Updates,
    kind: SettingKind::Int {
        min: 5,
        max: 240,
        default: 30,
    },
    label: "setting.update.idle_minutes",
    help: "setting.update.idle_minutes.help",
    protected: true,
    restart: false,
};

pub(super) const RETENTION_DAYS: SettingSpec = SettingSpec {
    key: UPDATE_RETENTION_KEY,
    page: SettingPage::Updates,
    kind: SettingKind::Int {
        min: 0,
        max: 3650,
        default: 14,
    },
    label: "setting.update.backup_retention_days",
    help: "setting.update.backup_retention_days.help",
    protected: true,
    restart: false,
};
