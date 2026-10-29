use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use tracing_subscriber::layer::SubscriberExt as _;

use super::*;

#[test]
fn a_storm_is_reached_once() {
    let start = Instant::now();
    let mut storm = Storm::default();
    let reached: Vec<bool> = (0..STORM + 50).map(|_| storm.seen(start)).collect();
    assert_eq!(reached.iter().filter(|reached| **reached).count(), 1);
    assert!(reached[usize::try_from(STORM).expect("small") - 1]);
}

#[test]
fn failed_presents_spread_over_time_are_no_storm() {
    let start = Instant::now();
    let mut storm = Storm::default();
    let step = WINDOW / (STORM / 2);
    let reached = (0..STORM * 4).any(|index| storm.seen(start + step * index));
    assert!(!reached);
}

/// The line comes from iced through the `log` bridge; any other error counts nothing.
#[test]
fn the_watch_tells_once_of_iced_s_storm() {
    let told = Arc::new(AtomicUsize::new(0));
    let watch = Watch::new({
        let told = Arc::clone(&told);
        move || {
            told.fetch_add(1, Ordering::Relaxed);
        }
    });
    let subscriber = tracing_subscriber::registry().with(watch);
    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..STORM * 2 {
            tracing::error!("the retention sweep failed");
        }
        assert_eq!(told.load(Ordering::Relaxed), 0);
        for _ in 0..STORM * 2 {
            tracing::error!("Error Other when presenting surface.");
        }
    });
    assert_eq!(told.load(Ordering::Relaxed), 1);
}
