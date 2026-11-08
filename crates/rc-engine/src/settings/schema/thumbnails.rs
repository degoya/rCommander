//! The thumbnails view (RC-100-89), on the Appearance page's Thumbnails tab, as Total Commander's
//! Configuration > Thumbnails: the tile size (`ThumbWidthNoCache`, 96 px), the cache on disk
//! (`ThumbsLocation`, empty for none) and its size, whether files on servers, devices and in
//! archives get one, and Explorer's thumbnails for the files rCommander does not decode itself
//! (`ThumbOptions` bit 1). None is protected: `panel.thumbnail_zoom` writes the size.

use super::{SettingKind, SettingPage, SettingSpec};

/// The side of a tile, in pixels at 100 %.
pub const THUMBNAIL_SIZE_KEY: &str = "thumbnails.size";
/// Whether finished tiles of local files are kept on disk, in the user's cache folder.
pub const THUMBNAIL_DISK_CACHE_KEY: &str = "thumbnails.disk_cache";
/// The most the cache on disk may hold, in MiB.
pub const THUMBNAIL_CACHE_MB_KEY: &str = "thumbnails.cache_mb";
/// Whether files over SFTP, FTP, on devices (MTP) and in archives get a thumbnail.
pub const THUMBNAIL_REMOTE_KEY: &str = "thumbnails.remote";
/// Whether Explorer's thumbnail handlers make the thumbnails rCommander does not decode itself
/// (Windows only).
pub const THUMBNAIL_EXPLORER_KEY: &str = "thumbnails.explorer";

/// The smallest and the largest tile.
pub const MIN_THUMBNAIL_SIZE: i64 = 32;
pub const MAX_THUMBNAIL_SIZE: i64 = 512;
/// Total Commander's default.
pub const DEFAULT_THUMBNAIL_SIZE: i64 = 96;

/// 96 pixels, as Total Commander.
pub(super) const SIZE: SettingSpec = SettingSpec {
    key: THUMBNAIL_SIZE_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: MIN_THUMBNAIL_SIZE,
        max: MAX_THUMBNAIL_SIZE,
        default: DEFAULT_THUMBNAIL_SIZE,
    },
    label: "setting.thumbnails.size",
    help: "setting.thumbnails.size.help",
    protected: false,
    restart: false,
};

/// On by default, as Total Commander keeps its thumbnail database.
pub(super) const DISK_CACHE: SettingSpec = SettingSpec {
    key: THUMBNAIL_DISK_CACHE_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: true },
    label: "setting.thumbnails.disk_cache",
    help: "setting.thumbnails.disk_cache.help",
    protected: false,
    restart: false,
};

/// 256 MiB; trimmed by age at the start.
pub(super) const CACHE_MB: SettingSpec = SettingSpec {
    key: THUMBNAIL_CACHE_MB_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Int {
        min: 16,
        max: 4096,
        default: 256,
    },
    label: "setting.thumbnails.cache_mb",
    help: "setting.thumbnails.cache_mb.help",
    protected: false,
    restart: false,
};

/// Off by default: a thumbnail reads the whole file, which costs a transfer on a server.
pub(super) const REMOTE: SettingSpec = SettingSpec {
    key: THUMBNAIL_REMOTE_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: false },
    label: "setting.thumbnails.remote",
    help: "setting.thumbnails.remote.help",
    protected: false,
    restart: false,
};

/// Off by default (owner, 2026-11-07): Explorer's handlers run other programs' code inside
/// rCommander, on files nobody vouches for.
pub(super) const EXPLORER: SettingSpec = SettingSpec {
    key: THUMBNAIL_EXPLORER_KEY,
    page: SettingPage::Appearance,
    kind: SettingKind::Bool { default: false },
    label: "setting.thumbnails.explorer",
    help: "setting.thumbnails.explorer.help",
    protected: false,
    restart: false,
};
