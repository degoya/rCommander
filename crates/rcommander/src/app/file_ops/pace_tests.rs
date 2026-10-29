use std::time::{Duration, Instant};

use super::*;

#[test]
fn the_speed_is_the_batch_s_bytes_and_files_over_the_window() {
    let start = Instant::now();
    let mut pace = Pace::new(start);
    assert_eq!(pace.rates(), None);
    pace.sample(start, 0, 0);
    assert_eq!(pace.rates(), None, "one sample is no speed");
    pace.sample(start + Duration::from_secs(2), 2_000, 4);
    assert_eq!(
        pace.rates(),
        Some(Rates {
            bytes: 1_000.0,
            files: 2.0
        })
    );
    assert_eq!(pace.elapsed(start + Duration::from_secs(7)), 7);
    pace.reset();
    assert_eq!(pace.rates(), None);
    assert_eq!(pace.elapsed(start + Duration::from_secs(8)), 8, "kept");
}

#[test]
fn the_time_left_is_the_longer_of_bytes_and_files() {
    let rates = Rates {
        bytes: 1_000.0,
        files: 2.0,
    };
    // Few big files: the bytes take longer.
    assert_eq!(time_left(10_000, 2, rates), Some(10));
    // Many small files: the files do.
    assert_eq!(time_left(1_000, 1_000, rates), Some(500));
    let bytes_only = Rates {
        bytes: 1_000.0,
        files: 0.0,
    };
    assert_eq!(time_left(3_000, 5, bytes_only), Some(3));
    let still = Rates {
        bytes: 0.0,
        files: 0.0,
    };
    assert_eq!(time_left(3_000, 5, still), None);
}
