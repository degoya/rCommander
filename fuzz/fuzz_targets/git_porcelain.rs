//! git's status of a folder (`rc_engine::git::parse_porcelain`, RC-100-97): the answer of
//! `git status --porcelain=v2 -z` about a folder anyone may have prepared. It is an error or the
//! paths with their changes, never a panic; no path is empty.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rc_engine::git::parse_porcelain;

fuzz_target!(|data: &[u8]| {
    if let Ok(status) = parse_porcelain(data) {
        for item in status.items {
            assert!(!item.path.is_empty(), "an empty path");
        }
    }
});
