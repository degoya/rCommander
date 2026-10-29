use std::time::{Duration, Instant};

use super::{MAX_PAGED, PAGE_TTL, PageKey, Pages};

fn key(offset: usize) -> PageKey {
    PageKey {
        fingerprint: 7,
        guarded: true,
        offset,
    }
}

#[test]
fn a_kept_search_goes_on_once_and_only_for_its_page() {
    let now = Instant::now();
    let mut pages = Pages::default();
    let id = pages.keep(key(5), "walk", now);
    for other in [
        key(10),
        PageKey {
            fingerprint: 8,
            ..key(5)
        },
        PageKey {
            guarded: false,
            ..key(5)
        },
    ] {
        assert_eq!(pages.take(id, other, now), None, "{other:?}");
    }
    assert_eq!(pages.take(id + 1, key(5), now), None, "another id");
    assert_eq!(pages.take(id, key(5), now), Some("walk"));
    assert_eq!(pages.take(id, key(5), now), None, "taken out");
}

#[test]
fn a_search_waits_for_its_next_page_up_to_its_time() {
    let now = Instant::now();
    let mut pages = Pages::default();
    let early = pages.keep(key(5), "early", now);
    let late = pages.keep(key(5), "late", now);
    let just_before = now + PAGE_TTL - Duration::from_millis(1);
    assert_eq!(pages.take(early, key(5), just_before), Some("early"));
    assert_eq!(pages.take(late, key(5), now + PAGE_TTL), None, "it went");
}

#[test]
fn the_search_used_longest_ago_goes_first() {
    let now = Instant::now();
    let mut pages = Pages::default();
    let ids: Vec<u64> = (0..=MAX_PAGED)
        .map(|n| pages.keep(key(5), n, now))
        .collect();
    assert_eq!(pages.take(ids[0], key(5), now), None, "the oldest went");
    for (n, id) in ids.iter().enumerate().skip(1) {
        assert_eq!(pages.take(*id, key(5), now), Some(n));
    }
}
