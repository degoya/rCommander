//! How many keys of the SSH agents one login offers (RC-080-43): the connection's own key first,
//! then the others, at most this many. A server counts every key against its `MaxAuthTries` and
//! learns each one offered, so the limit is low by default. Protected: it bounds what a server
//! learns of a person's keys, which is a person's choice, never an agent's. On the Terminal page,
//! with the logins.

use super::{SettingKind, SettingPage, SettingSpec};

/// The most agent keys one login offers.
pub const AGENT_MAX_KEYS_KEY: &str = "ssh.agent_max_keys";

pub(super) const AGENT_MAX_KEYS: SettingSpec = SettingSpec {
    key: AGENT_MAX_KEYS_KEY,
    page: SettingPage::Terminal,
    // `rc_remote::ssh::MAX_AGENT_KEYS` and `DEFAULT_MAX_AGENT_KEYS`, held equal by the tests.
    kind: SettingKind::Int {
        min: 1,
        max: 20,
        default: 5,
    },
    label: "setting.ssh.agent_max_keys",
    help: "setting.ssh.agent_max_keys.help",
    protected: true,
    restart: false,
};

#[cfg(test)]
#[path = "ssh_agent_tests.rs"]
mod tests;
