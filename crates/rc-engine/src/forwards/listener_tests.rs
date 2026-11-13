//! The pause and the one log line of a listener whose `accept` keeps failing (RC-110-26).

use std::time::Duration;

use super::{ACCEPT_PAUSE_MAX, ACCEPT_PAUSE_MIN, AcceptFailures};

#[test]
fn a_run_of_failures_is_logged_once_and_waits_from_50_to_500_ms() {
    let mut failures = AcceptFailures::default();
    let run: Vec<(bool, Duration)> = (0..6).map(|_| failures.failed()).collect();
    assert_eq!(
        run,
        [
            (true, Duration::from_millis(50)),
            (false, Duration::from_millis(100)),
            (false, Duration::from_millis(200)),
            (false, Duration::from_millis(400)),
            (false, Duration::from_millis(500)),
            (false, Duration::from_millis(500)),
        ]
    );
    assert_eq!((ACCEPT_PAUSE_MIN, ACCEPT_PAUSE_MAX), (run[0].1, run[5].1));
}

#[test]
fn a_failure_that_stays_never_waits_less_or_longer_than_the_bounds() {
    let mut failures = AcceptFailures::default();
    for _ in 0..10_000 {
        let (_, pause) = failures.failed();
        assert!((ACCEPT_PAUSE_MIN..=ACCEPT_PAUSE_MAX).contains(&pause));
    }
}

#[test]
fn an_accepted_connection_ends_the_run() {
    let mut failures = AcceptFailures::default();
    failures.failed();
    failures.failed();
    failures.accepted();
    assert_eq!(failures.failed(), (true, Duration::from_millis(50)));
}
