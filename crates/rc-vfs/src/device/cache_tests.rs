use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::{FRESH, Lookup, ObjectCache, same_name};
use crate::device::Object;

fn object(id: &str, name: &str) -> Object {
    Object {
        id: id.to_owned(),
        name: name.to_owned(),
        folder: false,
        size: 1,
        modified: None,
    }
}

#[test]
fn the_oldest_path_goes_when_the_cache_is_full() {
    let mut cache = ObjectCache::new(2);
    cache.put(PathBuf::from("/s/a"), object("1", "a"));
    cache.put(PathBuf::from("/s/b"), object("2", "b"));
    cache.put(PathBuf::from("/s/c"), object("3", "c"));
    assert_eq!(cache.len(), 2, "never more than its capacity");
    assert!(cache.get(Path::new("/s/a")).is_none(), "the oldest went");
    assert!(cache.get(Path::new("/s/c")).is_some());
}

#[test]
fn putting_a_path_again_does_not_count_twice() {
    let mut cache = ObjectCache::new(2);
    cache.put(PathBuf::from("/s/a"), object("1", "a"));
    cache.put(PathBuf::from("/s/a"), object("1", "a"));
    cache.put(PathBuf::from("/s/b"), object("2", "b"));
    assert_eq!(cache.len(), 2);
    assert!(cache.get(Path::new("/s/a")).is_some());
}

#[test]
fn forgetting_a_path_forgets_everything_below_it() {
    let mut cache = ObjectCache::new(10);
    cache.put(PathBuf::from("/s/dir"), object("1", "dir"));
    cache.put(PathBuf::from("/s/dir/x"), object("2", "x"));
    cache.put(PathBuf::from("/s/dirt"), object("3", "dirt"));
    cache.forget(Path::new("/s/dir"));
    assert!(cache.get(Path::new("/s/dir")).is_none());
    assert!(cache.get(Path::new("/s/dir/x")).is_none());
    assert!(cache.get(Path::new("/s/dirt")).is_some(), "a sibling stays");
}

#[test]
fn a_listed_folder_answers_for_names_it_does_not_hold() {
    let now = Instant::now();
    let mut cache = ObjectCache::new(10);
    let dir = Path::new("/s");
    assert_eq!(cache.lookup(dir, "a", now), Lookup::Unknown);
    cache.replace_children(dir, &[object("1", "Photo.JPG")], now);
    assert_eq!(cache.lookup(dir, "missing", now), Lookup::Absent);
    assert_eq!(
        cache.lookup(dir, "photo.jpg", now),
        Lookup::Found(object("1", "Photo.JPG")),
        "found in another case"
    );
    let later = now + FRESH + Duration::from_secs(1);
    assert_eq!(
        cache.lookup(dir, "missing", later),
        Lookup::Unknown,
        "an old listing no longer answers"
    );
}

#[test]
fn a_listing_replaces_what_was_kept_of_the_folder() {
    let now = Instant::now();
    let mut cache = ObjectCache::new(10);
    let dir = Path::new("/s");
    cache.replace_children(dir, &[object("1", "gone"), object("2", "stays")], now);
    cache.put(PathBuf::from("/s/stays/inner"), object("9", "inner"));
    cache.replace_children(dir, &[object("2", "stays"), object("3", "new")], now);
    assert!(
        cache.get(Path::new("/s/gone")).is_none(),
        "deleted on the device"
    );
    assert!(cache.get(Path::new("/s/new")).is_some());
    assert!(
        cache.get(Path::new("/s/stays/inner")).is_some(),
        "the same object keeps what is below it"
    );
    cache.replace_children(dir, &[object("4", "stays")], now);
    assert!(
        cache.get(Path::new("/s/stays/inner")).is_none(),
        "another object under the name does not"
    );
}

#[test]
fn a_folder_with_an_object_pushed_out_is_no_longer_complete() {
    let now = Instant::now();
    let mut cache = ObjectCache::new(2);
    let dir = Path::new("/s");
    cache.replace_children(
        dir,
        &[object("1", "a"), object("2", "b"), object("3", "c")],
        now,
    );
    assert_eq!(
        cache.lookup(dir, "a", now),
        Lookup::Unknown,
        "`a` was pushed out: the device is asked"
    );
}

#[test]
fn names_are_the_same_in_any_case() {
    assert!(same_name("DCIM", "dcim"));
    assert!(same_name("\u{c4}rger.txt", "\u{e4}rger.TXT"));
    assert!(!same_name("a", "b"));
}
