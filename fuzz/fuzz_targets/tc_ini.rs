//! Total Commander's INI files (`rc_engine::import::tc_ini`): the bytes decoded as the import
//! decodes them, then parsed. Every section a header names is found again, every key of it is,
//! and no value kept is longer than `MAX_VALUE_CHARS`.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rc_engine::import::tc_ini::{MAX_VALUE_CHARS, decode, parse};

/// Section headers checked at most, so one input of many headers stays quick.
const MAX_HEADERS: usize = 256;

fuzz_target!(|data: &[u8]| {
    let text = decode(data);
    let ini = parse(&text);
    let headers = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix('['))
        .filter_map(|rest| rest.split_once(']'))
        .map(|(name, _)| name.trim())
        .take(MAX_HEADERS);
    for name in headers {
        let section = ini
            .section(name)
            .unwrap_or_else(|| panic!("the section [{name}] was parsed but is not found"));
        for (key, value) in section.entries() {
            assert!(
                value.chars().count() <= MAX_VALUE_CHARS,
                "[{name}] {key} kept a value of {} characters",
                value.chars().count()
            );
            assert!(section.get(key).is_some(), "[{name}] {key} is not found");
        }
    }
});
