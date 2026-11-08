//! The tiles of local files on disk, so a folder shows them at once the next time
//! (`thumbnails.disk_cache`, Total Commander's `ThumbsLocation`). One PNG per tile in the user's
//! cache folder (`rcommander/thumbnails`), the folder the user's alone (0700 on Unix), each named
//! by [`ThumbKey::disk_name`] - a hash, never a path. Not in `rcommander.db`: a cache is no
//! durable state (AGENTS.md).
//!
//! A tile read back is a file anyone with the user's rights could have put there: it is decoded
//! with the picture limits and must fit its tile, or it is deleted. The start trims the cache by
//! age to 90 % of `thumbnails.cache_mb` once it is over; the settings clear it. Every call
//! blocks: run it off the UI thread.

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::SystemTime,
};

use ::image::{ImageFormat as Codec, RgbaImage};
use rc_engine::viewer::ImageFormat;

use super::{decode::Tile, key::ThumbKey};
use crate::app::viewer::picture;

/// The largest tile file read back: a 512-pixel tile is 1 MiB of pixels before compression.
const MAX_TILE_FILE: u64 = 4 * 1024 * 1024;

/// A folder of tiles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiskCache {
    dir: PathBuf,
}

impl DiskCache {
    /// The cache in `dir`.
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The user's cache: `rcommander/thumbnails` in the cache folder (`%LOCALAPPDATA%` on
    /// Windows); `None` without a home.
    pub fn user() -> Option<Self> {
        let base = directories::BaseDirs::new()?;
        Some(Self::at(
            base.cache_dir().join("rcommander").join("thumbnails"),
        ))
    }

    fn file_of(&self, key: &ThumbKey) -> PathBuf {
        self.dir.join(key.disk_name())
    }

    /// The tile of `key`, when the cache has a valid one: a PNG within the limits that fits its
    /// tile. Anything else under its name is deleted. A tile read counts as used now, for the
    /// trimming by age.
    pub fn read(&self, key: &ThumbKey) -> Option<Tile> {
        let path = self.file_of(key);
        let found = fs::symlink_metadata(&path).ok()?;
        let tile = if found.is_file() && found.len() <= MAX_TILE_FILE {
            fs::read(&path)
                .ok()
                .and_then(|bytes| valid(&bytes, key.tile))
        } else {
            None
        };
        match &tile {
            Some(_) => touch(&path),
            None => remove(&path),
        }
        tile
    }

    /// Keeps `tile` as the tile of `key`: written beside and renamed, so a reader never sees half
    /// a file.
    pub fn write(&self, key: &ThumbKey, tile: &Tile) -> io::Result<()> {
        private_dir(&self.dir)?;
        let image = RgbaImage::from_raw(tile.width, tile.height, tile.rgba.clone())
            .ok_or_else(|| io::Error::other("the tile's pixels do not match its size"))?;
        let mut bytes = Vec::new();
        image
            .write_to(&mut io::Cursor::new(&mut bytes), Codec::Png)
            .map_err(io::Error::other)?;
        let path = self.file_of(key);
        let part = path.with_extension(format!("part{}", std::process::id()));
        fs::write(&part, bytes)?;
        fs::rename(&part, &path).inspect_err(|_| remove(&part))
    }

    /// The bytes the cache's tiles take.
    pub fn size(&self) -> u64 {
        self.tiles().iter().map(|(_, len, _)| len).sum()
    }

    /// Trims the cache once it holds more than `limit` bytes: the tiles used longest ago go
    /// until it holds 90 % of it. The bytes left.
    pub fn trim(&self, limit: u64) -> u64 {
        let mut tiles = self.tiles();
        let mut total: u64 = tiles.iter().map(|(_, len, _)| len).sum();
        if total <= limit {
            return total;
        }
        let target = limit / 10 * 9;
        tiles.sort_by_key(|(_, _, used)| *used);
        for (path, len, _) in tiles {
            if total <= target {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
        total
    }

    /// Deletes every tile; files of other names stay. The bytes left.
    pub fn clear(&self) -> u64 {
        for (path, _, _) in self.tiles() {
            remove(&path);
        }
        self.size()
    }

    /// The cache's tiles: their paths, sizes and last use. Only files named as tiles count.
    fn tiles(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let Ok(read) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        read.filter_map(Result::ok)
            .filter(|item| is_tile_name(&item.file_name().to_string_lossy()))
            .filter_map(|item| {
                let meta = item.metadata().ok().filter(fs::Metadata::is_file)?;
                let used = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                Some((item.path(), meta.len(), used))
            })
            .collect()
    }
}

/// Whether `name` is a tile's file name: 64 hex digits and `.png`.
fn is_tile_name(name: &str) -> bool {
    name.strip_suffix(".png")
        .is_some_and(|hash| hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// `bytes` as a tile of `side`: a PNG within the picture limits, no side above the tile's.
fn valid(bytes: &[u8], side: u32) -> Option<Tile> {
    let pixels = picture::decode(bytes, ImageFormat::Png, u64::MAX).ok()?;
    let fits =
        (1..=side).contains(&pixels.shown_width) && (1..=side).contains(&pixels.shown_height);
    fits.then_some(Tile {
        width: pixels.shown_width,
        height: pixels.shown_height,
        rgba: pixels.rgba,
    })
}

fn remove(path: &Path) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::debug!(%error, "a thumbnail file was not removed");
    }
}

/// The tile was used now: its time moves, so the trimming keeps it longest.
fn touch(path: &Path) {
    let touched = fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(SystemTime::now()));
    if let Err(error) = touched {
        tracing::debug!(%error, "a thumbnail's time was not moved");
    }
}

/// Creates `dir` for the user alone (0700 on Unix; on Windows the profile's folder is already
/// the user's).
fn private_dir(dir: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        let found = fs::symlink_metadata(dir)?;
        if !found.file_type().is_dir() {
            return Err(io::Error::other("the thumbnail folder is not a folder"));
        }
        if found.permissions().mode() & 0o077 != 0 {
            fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(dir)
    }
}

#[cfg(test)]
#[path = "disk_tests.rs"]
mod tests;
