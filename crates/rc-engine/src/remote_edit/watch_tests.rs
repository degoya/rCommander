use std::time::{Duration, SystemTime};

use super::{POLL, Watch};
use crate::remote_edit::Stamp;

fn stamp(size: u64, second: u64) -> Option<Stamp> {
    Some(Stamp {
        size,
        modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(second)),
    })
}

/// A clock the test moves: each read happens at the next multiple of [`POLL`].
struct Clock {
    now: Duration,
}

impl Clock {
    fn tick(&mut self) -> Duration {
        self.now += POLL;
        self.now
    }
}

#[test]
fn a_save_is_seen_within_two_seconds() {
    let mut watch = Watch::new(stamp(10, 0));
    let mut clock = Clock {
        now: Duration::ZERO,
    };
    // Saved just after the read at 0 s.
    let saved_at = Duration::from_millis(10);
    let mut seen_at = None;
    for _ in 0..5 {
        let now = clock.tick();
        if watch.observe(stamp(12, 1)) {
            seen_at = Some(now);
            break;
        }
    }
    let seen_at = seen_at.expect("the save is seen");
    assert!(seen_at - saved_at <= Duration::from_secs(2), "{seen_at:?}");
    assert_eq!(watch.synced(), stamp(12, 1));
}

#[test]
fn a_stand_that_holds_for_one_read_starts_nothing() {
    let mut watch = Watch::new(stamp(10, 0));
    // Half written at the first read, whole at the next ones.
    assert!(!watch.observe(stamp(4, 1)));
    assert!(!watch.observe(stamp(12, 1)));
    assert!(watch.observe(stamp(12, 1)), "the whole file is seen");
    assert_eq!(watch.synced(), stamp(12, 1), "never the half one");
}

#[test]
fn an_unchanged_copy_and_a_missing_one_start_nothing() {
    let mut watch = Watch::new(stamp(10, 0));
    for _ in 0..3 {
        assert!(!watch.observe(stamp(10, 0)));
    }
    // An editor that saves through a temporary name: gone for a read, then back as before.
    assert!(!watch.observe(None));
    assert!(!watch.observe(stamp(10, 0)));
    // Gone between two reads of a new stamp: the next read starts the count anew.
    assert!(!watch.observe(stamp(11, 2)));
    assert!(!watch.observe(None));
    assert!(!watch.observe(stamp(11, 2)));
    assert!(watch.observe(stamp(11, 2)));
}

#[test]
fn each_save_is_seen_once() {
    let mut watch = Watch::new(stamp(10, 0));
    let seen: usize = [11, 11, 11, 11, 12, 12, 12]
        .into_iter()
        .map(|size| usize::from(watch.observe(stamp(size, size))))
        .sum();
    assert_eq!(seen, 2);
}
