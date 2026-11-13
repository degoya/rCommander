//! The tiles on disk: read back only when valid, a changed file a miss, trimmed by age.

use std::{
    fs,
    sync::atomic::AtomicU32,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rc_engine::VfsPath;

use super::{DiskCache, MAX_TILE_FILE};
use crate::app::thumbnails::{decode::Tile, key::ThumbKey};

fn key(name: &str, seconds: u64) -> ThumbKey {
    ThumbKey {
        path: VfsPath::local("/photos").join(name),
        size: 1000,
        modified: Some(UNIX_EPOCH + Duration::from_secs(seconds)),
        tile: 96,
    }
}

/// A tile of `width` by `height` in a pattern, so its PNG does not shrink to nothing.
fn tile(width: u32, height: u32) -> Tile {
    let rgba = (0..width * height * 4)
        .map(|index| (index.wrapping_mul(2_654_435_761) >> 24) as u8)
        .collect();
    Tile {
        width,
        height,
        rgba,
    }
}

fn cache() -> (tempfile::TempDir, DiskCache) {
    let root = tempfile::tempdir().expect("temp dir");
    let cache = DiskCache::at(root.path().join("thumbnails"));
    (root, cache)
}

#[test]
fn a_tile_comes_back_and_a_changed_file_misses() {
    let (_root, cache) = cache();
    let written = tile(96, 64);
    cache.write(&key("a.jpg", 1), &written).expect("written");
    assert_eq!(cache.read(&key("a.jpg", 1)), Some(written));
    assert_eq!(cache.read(&key("a.jpg", 2)), None, "the file changed since");
    assert!(cache.size() > 0);
}

#[test]
fn a_foreign_or_oversized_file_is_discarded() {
    let (_root, cache) = cache();
    cache.write(&key("a.jpg", 1), &tile(8, 8)).expect("written");
    let path = cache.file_of(&key("a.jpg", 1));
    fs::write(&path, b"not a picture at all").expect("written");
    assert_eq!(cache.read(&key("a.jpg", 1)), None);
    assert!(!path.exists(), "removed");

    // A picture larger than the tile is no tile of this size.
    cache
        .write(&key("b.jpg", 1), &tile(200, 200))
        .expect("written");
    assert_eq!(cache.read(&key("b.jpg", 1)), None);
    assert!(!cache.file_of(&key("b.jpg", 1)).exists());

    let huge = cache.file_of(&key("c.jpg", 1));
    let file = fs::File::create(&huge).expect("created");
    file.set_len(MAX_TILE_FILE + 1).expect("grown");
    drop(file);
    assert_eq!(cache.read(&key("c.jpg", 1)), None);
    assert!(!huge.exists());
}

#[test]
fn trimming_keeps_the_newest_tiles_within_the_limit() {
    let (_root, cache) = cache();
    let start = SystemTime::now() - Duration::from_secs(3600);
    for index in 0..10u64 {
        let key = key(&format!("{index}.jpg"), index);
        cache.write(&key, &tile(64, 64)).expect("written");
        let file = fs::File::options()
            .write(true)
            .open(cache.file_of(&key))
            .expect("open");
        file.set_modified(start + Duration::from_secs(index * 60))
            .expect("time");
    }
    let total = cache.size();
    assert_eq!(cache.trim(total), total, "within the limit nothing goes");
    let limit = total / 2;
    let left = cache.trim(limit);
    assert!(left <= limit / 10 * 9, "{left} of {limit}");
    assert_eq!(left, cache.size());
    assert!(cache.read(&key("9.jpg", 9)).is_some(), "the newest stays");
    assert!(cache.read(&key("0.jpg", 0)).is_none(), "the oldest went");
}

#[test]
fn clearing_takes_only_the_tiles() {
    let (root, cache) = cache();
    cache.write(&key("a.jpg", 1), &tile(8, 8)).expect("written");
    let other = root.path().join("thumbnails").join("keep.txt");
    fs::write(&other, b"x").expect("written");
    assert_eq!(cache.clear(), 0);
    assert!(other.exists(), "not a tile's name");
}

#[cfg(unix)]
#[test]
fn the_folder_is_the_users_alone() {
    use std::os::unix::fs::PermissionsExt as _;
    let (root, cache) = cache();
    cache.write(&key("a.jpg", 1), &tile(8, 8)).expect("written");
    let mode = fs::metadata(root.path().join("thumbnails"))
        .expect("folder")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

/// RC-110-29: the cache is trimmed while the program runs, at every `TRIM_EVERY`th tile written.
#[test]
fn the_cache_is_trimmed_as_tiles_are_written() {
    let (_root, cache) = cache();
    let writes = AtomicU32::new(0);
    let mut trims = Vec::new();
    for index in 0..u64::from(DiskCache::TRIM_EVERY) {
        let key = key(&format!("{index}.jpg"), index);
        cache.write(&key, &tile(16, 16)).expect("written");
        trims.extend(cache.written(&writes, 0));
    }
    assert_eq!(trims, [0], "one trim, at the last write, to the limit");
    assert_eq!(cache.size(), 0);
}
