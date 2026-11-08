//! Everything's answer to a search (`rc_engine::search::everything::parse_reply`, RC-100-93): a
//! block another process wrote, read with every offset and length checked. It is an error or the
//! paths, never a panic; never more paths than allowed, none longer than Windows' long paths.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rc_engine::search::everything::parse_reply;

/// Paths taken at most here, so one input stays quick.
const MAX: usize = 1024;

fuzz_target!(|data: &[u8]| {
    if let Ok(paths) = parse_reply(data, MAX) {
        assert!(
            paths.len() <= MAX,
            "{} paths for at most {MAX}",
            paths.len()
        );
        for path in paths {
            let units = path.to_string_lossy().encode_utf16().count();
            assert!(units <= 32_767, "a path of {units} units");
        }
    }
});
