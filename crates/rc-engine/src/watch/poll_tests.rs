//! The fallback's rule on a clock the test moves.

use std::time::{Duration, Instant, SystemTime};

use super::{EVERY, Poller};

#[test]
fn a_changed_time_is_one_reading_an_unchanged_one_none() {
    let start = Instant::now();
    let before = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000);
    let after = before + Duration::from_secs(1);
    let mut poller = Poller::new(Some(before), start);
    assert!(!poller.seen(Some(before), start + EVERY), "unchanged");
    assert!(poller.seen(Some(after), start + EVERY * 2), "changed");
    assert!(!poller.seen(Some(after), start + EVERY * 3), "read once");
}

#[test]
fn a_folder_is_read_at_most_once_a_second() {
    let start = Instant::now();
    let mut poller = Poller::new(None, start);
    let mut stats = 0;
    // Asked every 10 ms for 5 s.
    for step in 0..500u32 {
        let now = start + Duration::from_millis(10) * step;
        if poller.wants_stat(now) {
            stats += 1;
            poller.seen(None, now);
        }
    }
    assert!(stats <= 5, "{stats} stats");
    assert!(stats >= 4, "{stats} stats");
}

#[test]
fn a_folder_that_stops_answering_changed() {
    let start = Instant::now();
    let mut poller = Poller::new(Some(SystemTime::UNIX_EPOCH), start);
    assert!(poller.seen(None, start + EVERY));
    assert_eq!(poller.next(), start + EVERY * 2);
}
