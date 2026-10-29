use std::time::{Duration, Instant};

use rc_vfs::VfsPath;

use super::{Meter, Progress, Throttle};

#[test]
fn a_file_s_bytes_start_again_with_each_file_and_with_each_attempt() {
    let mut progress = Progress {
        bytes_total: 300,
        ..Progress::default()
    };
    progress.begin_file(VfsPath::local("/a"), 100);
    progress.count(60);
    progress.count(40);
    assert_eq!(
        (progress.file_bytes_done, progress.file_bytes_total),
        (100, 100)
    );
    progress.begin_file(VfsPath::local("/b"), 200);
    assert_eq!(
        (progress.file_bytes_done, progress.file_bytes_total),
        (0, 200)
    );
    assert_eq!(progress.current, Some(VfsPath::local("/b")));
    progress.count(50);
    // A retry: the job's bytes go back to where the file began, the file's to 0.
    progress.restart_file(100);
    assert_eq!((progress.bytes_done, progress.file_bytes_done), (100, 0));
    progress.count(200);
    assert_eq!((progress.bytes_done, progress.file_bytes_done), (300, 200));
}

#[test]
fn the_speed_is_bytes_over_the_window() {
    let start = Instant::now();
    let mut meter = Meter::default();
    assert_eq!(meter.sample(start, 0), 0, "one sample is no speed");
    assert_eq!(meter.sample(start + Duration::from_secs(2), 2_000), 1_000);
    // Samples older than the window drop out: only the last 5 s count.
    assert_eq!(meter.sample(start + Duration::from_secs(8), 2_000), 0);
    meter.reset();
    assert_eq!(meter.sample(start + Duration::from_secs(9), 9_000), 0);
}

#[test]
fn the_time_left_follows_the_speed() {
    let start = Instant::now();
    let mut meter = Meter::default();
    let mut progress = Progress {
        bytes_total: 10_000,
        ..Progress::default()
    };
    progress.measure(&mut meter, start);
    assert_eq!(progress.eta_seconds, None);
    progress.bytes_done = 2_000;
    progress.measure(&mut meter, start + Duration::from_secs(1));
    assert_eq!(progress.bytes_per_second, 2_000);
    assert_eq!(progress.eta_seconds, Some(4));
}

#[test]
fn the_throttle_lets_one_event_through_per_interval() {
    let start = Instant::now();
    let mut throttle = Throttle::new(Duration::from_millis(100));
    assert!(throttle.ready(start));
    assert!(!throttle.ready(start + Duration::from_millis(50)));
    assert!(throttle.ready(start + Duration::from_millis(100)));
    let mut always = Throttle::new(Duration::ZERO);
    assert!(always.ready(start));
    assert!(always.ready(start));
}
