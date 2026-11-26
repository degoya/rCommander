use std::path::PathBuf;

use super::IconCache;
use crate::app::icons::key::IconKey;

fn ext(name: &str) -> IconKey {
    IconKey::Ext(name.into())
}

#[test]
fn a_key_is_asked_for_once_until_it_is_answered() {
    let mut cache = IconCache::<u32>::new(8);
    assert_eq!(cache.want(ext("txt")), Some(ext("txt")));
    assert_eq!(cache.want(ext("txt")), None, "pending: not asked for again");
    assert_eq!(cache.get(&ext("txt")), None);
    cache.fill(&ext("txt"), Some(7));
    assert_eq!(cache.get(&ext("txt")), Some(&7));
    assert_eq!(cache.want(ext("txt")), None);
}

#[test]
fn a_key_without_an_icon_is_not_asked_for_again() {
    let mut cache = IconCache::<u32>::new(8);
    cache.want(ext("xyz"));
    cache.fill(&ext("xyz"), None);
    assert_eq!(cache.get(&ext("xyz")), None);
    assert_eq!(cache.want(ext("xyz")), None);
}

#[test]
fn a_skipped_key_is_asked_for_again() {
    let mut cache = IconCache::<u32>::new(8);
    let file = IconKey::File(PathBuf::from("/bin/tool.exe"));
    cache.want(file.clone());
    cache.forget(&file);
    assert_eq!(cache.want(file.clone()), Some(file.clone()));
    // A loaded key is never forgotten.
    cache.fill(&file, Some(1));
    cache.forget(&file);
    assert_eq!(cache.get(&file), Some(&1));
}

#[test]
fn the_least_recently_wanted_key_goes_at_the_limit() {
    let mut cache = IconCache::<u32>::new(3);
    for (value, name) in ["a", "b", "c"].into_iter().enumerate() {
        cache.want(ext(name));
        cache.fill(&ext(name), Some(value as u32));
    }
    // "a" is shown again, so "b" is now the oldest.
    cache.want(ext("a"));
    assert_eq!(cache.want(ext("d")), Some(ext("d")));
    assert_eq!(cache.len(), 3);
    assert_eq!(cache.get(&ext("a")), Some(&0));
    assert_eq!(cache.get(&ext("b")), None, "evicted");
    assert_eq!(cache.get(&ext("c")), Some(&2));
    assert_eq!(cache.want(ext("b")), Some(ext("b")), "asked for anew");
}

#[test]
fn answers_after_a_clear_are_dropped() {
    let mut cache = IconCache::<u32>::new(8);
    cache.want(ext("txt"));
    cache.clear();
    cache.fill(&ext("txt"), Some(1));
    assert_eq!(cache.get(&ext("txt")), None);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.want(ext("txt")), Some(ext("txt")));
}

#[test]
fn the_eviction_takes_the_oldest_from_the_order_and_keeps_it_whole() {
    let mut cache = IconCache::<u32>::new(100);
    for number in 0..10_000 {
        let name = format!("e{number}");
        cache.want(ext(&name));
        cache.fill(&ext(&name), Some(number));
        if number % 7 == 0 {
            // A key shown again moves to the end of the order.
            cache.want(ext("e0"));
        }
    }
    cache.want(ext("e0"));
    assert_eq!(cache.len(), 100);
    let order = cache.oldest_first();
    assert_eq!(order.len(), 100, "one entry in the order per key");
    assert_eq!(
        order.last(),
        Some(&&ext("e0")),
        "the one shown last is the newest"
    );
    assert_eq!(cache.get(&ext("e0")), Some(&0), "kept by being shown");
    assert_eq!(order.first(), Some(&&ext("e9901")), "the oldest left");
    assert_eq!(cache.get(&ext("e9900")), None, "the oldest before it went");
    cache.forget(&ext("e9999"));
    assert_eq!(
        cache.oldest_first().len(),
        100,
        "a loaded key is not forgotten"
    );
    cache.clear();
    assert!(cache.oldest_first().is_empty());
}
