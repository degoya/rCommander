//! What a tile is kept by, and where an entry's tile comes from - if it gets one at all.

use std::time::{SystemTime, UNIX_EPOCH};

use rc_engine::{Entry, VfsPath, extension};
use rc_vfs::BackendId;

use super::settings::ThumbSettings;
use crate::app::viewer::picture;

/// The extensions of the pictures rCommander decodes itself (`picture::decode`): PNG, JPEG, GIF,
/// WebP, BMP and ICO. The content decides the decoder; a file that is none of them gets none.
const PICTURES: &[&str] = &[
    "png", "jpg", "jpeg", "jpe", "jfif", "gif", "webp", "bmp", "dib", "ico",
];

/// The most of a file on a server, a device or in an archive read for its tile.
pub const REMOTE_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// The version of the tiles' format on disk: a new one makes every old file a miss.
const DISK_FORMAT: &[u8] = b"rcommander-thumbnail-1\0";

/// One tile: a file as it was listed - its size and time, so a changed file gets a new one - at
/// one tile size.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbKey {
    pub path: VfsPath,
    pub size: u64,
    pub modified: Option<SystemTime>,
    /// The tile's side, in pixels.
    pub tile: u32,
}

/// Where a tile comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// rCommander's own decoder, within the picture limits.
    Picture,
    /// Explorer's thumbnail handlers (Windows, `thumbnails.explorer`).
    Explorer,
}

impl ThumbKey {
    /// The tile of `entry`, listed in `dir`, at `tile` pixels.
    pub fn of(entry: &Entry, dir: &VfsPath, tile: u32) -> Self {
        Self {
            path: dir.join(&entry.name),
            size: entry.size,
            modified: entry.modified,
            tile,
        }
    }

    /// Whether the file is on this machine (a share and WSL count as local, as in Total
    /// Commander): only those tiles go to the disk cache.
    pub fn local(&self) -> bool {
        self.path.backend == BackendId::Local
    }

    /// The name of the tile's file in the disk cache: a BLAKE3 hash of the format, the path, the
    /// size, the time and the tile size, so no path stands there in clear.
    pub fn disk_name(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(DISK_FORMAT);
        hasher.update(self.path.path.as_os_str().as_encoded_bytes());
        hasher.update(b"\0");
        hasher.update(&self.size.to_le_bytes());
        let modified = self
            .modified
            .map(|time| match time.duration_since(UNIX_EPOCH) {
                Ok(after) => i128::try_from(after.as_nanos()).unwrap_or(i128::MAX),
                Err(before) => -i128::try_from(before.duration().as_nanos()).unwrap_or(i128::MAX),
            });
        hasher.update(&[u8::from(modified.is_some())]);
        hasher.update(&modified.unwrap_or_default().to_le_bytes());
        hasher.update(&self.tile.to_le_bytes());
        format!("{}.png", hasher.finalize().to_hex())
    }
}

/// Where the tile of `entry` in `dir` comes from; `None` shows its icon. A folder has none; a
/// picture is decoded by rCommander; any other local file asks Explorer where that is switched
/// on (Windows only). A file on a server, a device or in an archive gets one only with
/// `thumbnails.remote`, and only a picture of at most [`REMOTE_MAX_BYTES`].
pub fn source(entry: &Entry, dir: &VfsPath, settings: &ThumbSettings) -> Option<Source> {
    if entry.is_dir_like() || entry.size == 0 {
        return None;
    }
    let local = dir.backend == BackendId::Local;
    if !local && !settings.remote {
        return None;
    }
    let limit = if local {
        picture::MAX_FILE_BYTES
    } else {
        REMOTE_MAX_BYTES
    };
    let ext = extension(entry).to_ascii_lowercase();
    if PICTURES.contains(&ext.as_str()) {
        return (entry.size <= limit).then_some(Source::Picture);
    }
    (local && settings.explorer && super::explorer::AVAILABLE).then_some(Source::Explorer)
}

#[cfg(test)]
#[path = "key_tests.rs"]
mod tests;
