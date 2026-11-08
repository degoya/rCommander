//! The thumbnails' settings (`thumbnails.*`, RC-100-89) as the window uses them: read at the start
//! and after each change.

use rc_db::Db;
use rc_engine::settings::{
    DEFAULT_THUMBNAIL_SIZE, MAX_THUMBNAIL_SIZE, MIN_THUMBNAIL_SIZE, THUMBNAIL_CACHE_MB_KEY,
    THUMBNAIL_DISK_CACHE_KEY, THUMBNAIL_EXPLORER_KEY, THUMBNAIL_REMOTE_KEY, THUMBNAIL_SIZE_KEY,
    stored_setting,
};
use serde_json::Value;

/// The settings' defaults, as the schema has them.
const DEFAULT_CACHE_MB: u64 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbSettings {
    /// The tile's side, in pixels.
    pub size: u32,
    pub disk_cache: bool,
    pub cache_mb: u64,
    pub remote: bool,
    pub explorer: bool,
}

impl Default for ThumbSettings {
    fn default() -> Self {
        Self {
            size: DEFAULT_THUMBNAIL_SIZE as u32,
            disk_cache: true,
            cache_mb: DEFAULT_CACHE_MB,
            remote: false,
            explorer: false,
        }
    }
}

impl ThumbSettings {
    /// The settings as stored in `db`; a value missing or out of its bounds is its default or
    /// its nearest bound.
    pub async fn read(db: &Db) -> Self {
        let defaults = Self::default();
        let int = |value: Option<Value>| value.as_ref().and_then(Value::as_i64);
        let flag = |value: Option<Value>, default| {
            value.as_ref().and_then(Value::as_bool).unwrap_or(default)
        };
        let size = int(stored_setting(db, THUMBNAIL_SIZE_KEY).await)
            .unwrap_or(DEFAULT_THUMBNAIL_SIZE)
            .clamp(MIN_THUMBNAIL_SIZE, MAX_THUMBNAIL_SIZE);
        let cache_mb = int(stored_setting(db, THUMBNAIL_CACHE_MB_KEY).await)
            .and_then(|mb| u64::try_from(mb).ok())
            .unwrap_or(defaults.cache_mb);
        Self {
            size: u32::try_from(size).unwrap_or(defaults.size),
            disk_cache: flag(
                stored_setting(db, THUMBNAIL_DISK_CACHE_KEY).await,
                defaults.disk_cache,
            ),
            cache_mb: cache_mb.clamp(16, 4096),
            remote: flag(stored_setting(db, THUMBNAIL_REMOTE_KEY).await, false),
            explorer: flag(stored_setting(db, THUMBNAIL_EXPLORER_KEY).await, false),
        }
    }

    /// The disk cache's limit, in bytes.
    pub fn cache_bytes(&self) -> u64 {
        self.cache_mb * 1024 * 1024
    }
}

/// Whether the setting `key` is one of the thumbnails'.
pub fn concerns(key: &str) -> bool {
    key.starts_with("thumbnails.")
}
