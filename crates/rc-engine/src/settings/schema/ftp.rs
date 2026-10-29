//! The FTP row (RC-080-46): whether a listing looks up what its links point at. Off, the
//! default, a listing shows links as links, at once; a link's target is looked up when it is
//! entered, opened, looked at or copied, and remembered for the session, so the next listing
//! shows a link to a folder as one. On, every listing looks up its links (at most 64) after the
//! transfer, one or two commands each, and sends them last. It holds for connections made after
//! the change.

use super::{SettingKind, SettingPage, SettingSpec};

/// Whether an FTP listing resolves its links.
pub const FTP_RESOLVE_LINKS_KEY: &str = "ftp.resolve_links_in_listing";

pub(super) const RESOLVE_LINKS: SettingSpec = SettingSpec {
    key: FTP_RESOLVE_LINKS_KEY,
    page: SettingPage::Transfers,
    kind: SettingKind::Bool { default: false },
    label: "setting.ftp.resolve_links_in_listing",
    help: "setting.ftp.resolve_links_in_listing.help",
    protected: false,
    restart: false,
};
