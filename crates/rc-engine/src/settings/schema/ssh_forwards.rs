//! Local port forwards of SSH connections (RC-110-03): whether one may listen on an address other
//! than loopback, whether a forward reconnects after its session was lost, and whether the status
//! bar counts the running ones. On the Terminal page, with the logins.
//!
//! `ssh.forwards_lan` is protected: a port other computers reach is a person's choice, never an
//! agent's. Off, a forward saved with such an address does not start (`forward_bind_refused`).

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether a forward may listen on an address other than `127.0.0.1` and `::1`.
pub const FORWARDS_LAN_KEY: &str = "ssh.forwards_lan";
/// Whether a forward's next connection connects its session again after a loss; off, a loss
/// stops the connection's forwards.
pub const FORWARD_RECONNECT_KEY: &str = "ssh.forward_reconnect";
/// Whether the status bar shows "Forwards: n" while some run.
pub const FORWARDS_IN_STATUS_BAR_KEY: &str = "ssh.forwards_in_status_bar";

pub(super) const FORWARDS_LAN: SettingSpec = SettingSpec {
    key: FORWARDS_LAN_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: false },
    label: "setting.ssh.forwards_lan",
    help: "setting.ssh.forwards_lan.help",
    protected: true,
    restart: false,
};

pub(super) const FORWARD_RECONNECT: SettingSpec = SettingSpec {
    key: FORWARD_RECONNECT_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.ssh.forward_reconnect",
    help: "setting.ssh.forward_reconnect.help",
    protected: false,
    restart: false,
};

pub(super) const FORWARDS_IN_STATUS_BAR: SettingSpec = SettingSpec {
    key: FORWARDS_IN_STATUS_BAR_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.ssh.forwards_in_status_bar",
    help: "setting.ssh.forwards_in_status_bar.help",
    protected: false,
    restart: false,
};
