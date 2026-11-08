//! The tiles in memory: asked for once, bounded by their bytes.

use rc_engine::VfsPath;

use super::TileCache;
use crate::app::thumbnails::key::ThumbKey;

fn key(name: &str) -> ThumbKey {
    ThumbKey {
        path: VfsPath::local("/photos").join(name),
        size: 1,
        modified: None,
        tile: 96,
    }
}

#[test]
fn a_key_is_asked_for_once_and_its_tile_kept() {
    let mut cache = TileCache::new(1000);
    assert!(cache.want(&key("a")));
    assert!(!cache.want(&key("a")), "pending: not asked again");
    assert_eq!(cache.get(&key("a")), None);
    cache.fill(&key("a"), Some(("tile a", 100)));
    assert_eq!(cache.get(&key("a")), Some(&"tile a"));
    assert_eq!(cache.held(), 100);
    assert!(cache.want(&key("b")));
    cache.fill(&key("b"), None);
    assert!(!cache.want(&key("b")), "a miss is not asked again");
    assert_eq!(cache.get(&key("b")), None);
}

#[test]
fn the_least_recently_shown_tiles_go_past_the_bytes() {
    let mut cache = TileCache::new(250);
    for name in ["a", "b", "c"] {
        cache.want(&key(name));
        cache.fill(&key(name), Some((name, 100)));
    }
    assert!(cache.held() <= 250);
    assert_eq!(cache.get(&key("a")), None, "the oldest went");
    assert!(cache.get(&key("b")).is_some() && cache.get(&key("c")).is_some());
    // Shown again, `b` is the newest: `c` goes next.
    cache.want(&key("b"));
    cache.want(&key("d"));
    cache.fill(&key("d"), Some(("d", 100)));
    assert_eq!(cache.get(&key("c")), None);
    assert!(cache.get(&key("b")).is_some());
}

#[test]
fn a_skipped_key_is_asked_again_and_a_late_answer_after_clear_is_dropped() {
    let mut cache = TileCache::new(1000);
    cache.want(&key("a"));
    cache.forget(&key("a"));
    assert!(cache.want(&key("a")), "asked again");
    cache.clear();
    cache.fill(&key("a"), Some(("late", 10)));
    assert_eq!(cache.get(&key("a")), None);
    assert_eq!((cache.len(), cache.held()), (0, 0));
}
