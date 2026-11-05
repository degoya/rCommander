//! The FTP row (RC-080-46): whether a listing looks up what its links point at. Off, the
//! default, a listing shows links as links, at once; a link's target is looked up when it is
//! entered, opened, looked at or copied, and remembered for the session, so the next listing
//! shows a link to a folder as one. On, every listing looks up its links (at most 64) after the
//! transfer, one or two commands each, and sends them last. It holds for connections made after
//! the change.
//!
//! Whether a plain FTP connection asks before its password crosses the network unencrypted
//! (RC-080-67): on, the default, the first login to such a server waits for a person - once, or
//! for the server until rCommander ends; off, it logs in at once, with the warning after it.
//! rCommander's own rule: Total Commander does not ask.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether an FTP listing resolves its links.
pub const FTP_RESOLVE_LINKS_KEY: &str = "ftp.resolve_links_in_listing";

/// Whether a plain FTP login asks before it sends the password in clear.
pub const FTP_ASK_PLAINTEXT_KEY: &str = "ftp.ask_plaintext";

pub(super) const ASK_PLAINTEXT: SettingSpec = SettingSpec {
    key: FTP_ASK_PLAINTEXT_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: true },
    label: "setting.ftp.ask_plaintext",
    help: "setting.ftp.ask_plaintext.help",
    protected: false,
    restart: false,
};

pub(super) const RESOLVE_LINKS: SettingSpec = SettingSpec {
    key: FTP_RESOLVE_LINKS_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: false },
    label: "setting.ftp.resolve_links_in_listing",
    help: "setting.ftp.resolve_links_in_listing.help",
    protected: false,
    restart: false,
};
