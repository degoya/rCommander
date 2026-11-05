//! An OpenSSH `known_hosts` file as the import reads it (`rc_remote::known_hosts_import::parse`).

#![no_main]

use libfuzzer_sys::fuzz_target;
use rc_remote::known_hosts_import::parse;

fuzz_target!(|data: &[u8]| {
    let parsed = parse(&String::from_utf8_lossy(data));
    std::hint::black_box(parsed);
});
