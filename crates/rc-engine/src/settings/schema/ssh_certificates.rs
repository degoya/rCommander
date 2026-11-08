//! OpenSSH user certificates at the SSH login (RC-060-09): whether a certificate a CA signed for a
//! key is used - the one stored with the key in the vault, the `<key>-cert.pub` beside a key file
//! (taken in with it when the key is imported), and those an SSH agent holds. On by default, as
//! OpenSSH's client does; off, keys log in alone. Protected: what a login presents to a server is
//! a person's choice, never an agent's. On the Terminal page, with the logins.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether OpenSSH user certificates are used.
pub const SSH_CERTIFICATES_KEY: &str = "ssh.certificates";

pub(super) const SSH_CERTIFICATES: SettingSpec = SettingSpec {
    key: SSH_CERTIFICATES_KEY,
    page: SettingPage::Terminal,
    kind: SettingKind::Bool { default: true },
    label: "setting.ssh.certificates",
    help: "setting.ssh.certificates.help",
    protected: true,
    restart: false,
};
