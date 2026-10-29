use std::time::{Duration, Instant};

use super::{FAILURE_WINDOW, MAX_CRASH_LIMIT, PluginHealth};
use crate::failure::codes;

#[test]
fn the_third_failure_in_five_minutes_suspends() {
    let health = PluginHealth::default();
    let start = Instant::now();
    assert!(!health.record_failure_at(start));
    assert!(!health.record_failure_at(start + Duration::from_secs(60)));
    assert!(health.check().is_ok());
    assert!(health.record_failure_at(start + Duration::from_secs(120)));
    assert!(health.is_suspended());
    assert_eq!(
        health.check().expect_err("suspended").code(),
        Some(codes::PLUGIN_SUSPENDED)
    );
    health.resume();
    assert!(health.check().is_ok(), "resumed, it runs again");
    assert!(
        !health.record_failure_at(start + Duration::from_secs(130)),
        "and counts from zero"
    );
}

#[test]
fn failures_older_than_the_window_do_not_count() {
    let health = PluginHealth::new(2);
    let start = Instant::now();
    assert!(!health.record_failure_at(start));
    assert!(!health.record_failure_at(start + FAILURE_WINDOW + Duration::from_secs(1)));
    assert!(!health.is_suspended());
}

#[test]
fn the_limit_is_held_to_its_range() {
    let health = PluginHealth::new(0);
    assert!(health.record_failure_at(Instant::now()), "at least one");
    let health = PluginHealth::new(1000);
    let start = Instant::now();
    for second in 0..u64::from(MAX_CRASH_LIMIT) - 1 {
        assert!(!health.record_failure_at(start + Duration::from_secs(second)));
    }
    assert!(health.record_failure_at(start + Duration::from_secs(20)));
}
