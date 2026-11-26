use std::{sync::Arc, time::Duration};

use super::{CopyWatch, StopOnDrop};

#[test]
fn a_fresh_watch_goes_on_and_counts_what_is_reported() {
    let watch = CopyWatch::new();
    assert!(watch.go_on());
    assert_eq!(watch.copied(), 0);
    watch.report(4096);
    assert_eq!(watch.copied(), 4096);
}

#[test]
fn a_cancelled_watch_stops_also_while_paused() {
    let watch = CopyWatch::new();
    watch.set_paused(true);
    watch.cancel();
    assert!(!watch.go_on());
}

#[test]
fn a_paused_watch_waits_until_it_is_let_go() {
    let watch = Arc::new(CopyWatch::new());
    watch.set_paused(true);
    let copier = {
        let watch = Arc::clone(&watch);
        std::thread::spawn(move || watch.go_on())
    };
    std::thread::sleep(Duration::from_millis(120));
    assert!(!copier.is_finished(), "the copy waits while paused");
    watch.set_paused(false);
    assert_eq!(copier.join().ok(), Some(true));
}

#[test]
fn dropping_the_guard_cancels_and_done_does_not() {
    let watch = Arc::new(CopyWatch::new());
    StopOnDrop::new(&watch).done();
    assert!(!watch.is_cancelled());
    drop(StopOnDrop::new(&watch));
    assert!(watch.is_cancelled());
}
