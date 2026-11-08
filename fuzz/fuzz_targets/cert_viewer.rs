//! A certificate or key file as the built-in viewer's certificate reader takes it apart, checks
//! it and reports it (`rc_cert::probe`, `rc_cert::read`, RC-110-08).

#![no_main]

use libfuzzer_sys::fuzz_target;
use rc_cert::checks::Clock;

fuzz_target!(|data: &[u8]| {
    let clock = Clock {
        now: 1_780_272_000,
        warn_days: 30,
    };
    std::hint::black_box(rc_cert::probe(data, data.len() as u64));
    std::hint::black_box(rc_cert::read(data, clock));
});
