//! A call that may hang (RC-110-29): given up after its time, counted until it returns, and not
//! started past the most that may hang.

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use super::within;

/// Until `running` is `count`, at most two seconds.
async fn until(running: &AtomicUsize, count: usize) {
    for _ in 0..200 {
        if running.load(Ordering::Relaxed) == count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("the count did not come to {count}");
}

#[tokio::test]
async fn a_call_that_hangs_is_given_up_and_counted_until_it_returns() {
    static RUNNING: AtomicUsize = AtomicUsize::new(0);
    let started = Instant::now();
    let hung = within(Duration::from_millis(20), &RUNNING, 1, || {
        std::thread::sleep(Duration::from_millis(400));
        Some(1)
    })
    .await;
    assert_eq!(hung, None, "given up");
    assert!(
        started.elapsed() < Duration::from_millis(350),
        "not waited for"
    );
    assert_eq!(RUNNING.load(Ordering::Relaxed), 1, "its thread still runs");
    let refused = within(Duration::from_secs(5), &RUNNING, 1, || Some(2)).await;
    assert_eq!(refused, None, "no further call while the most hang");
    until(&RUNNING, 0).await;
    let answered = within(Duration::from_secs(5), &RUNNING, 1, || Some(3)).await;
    assert_eq!(answered, Some(3), "asked again once it came back");
    until(&RUNNING, 0).await;
}
