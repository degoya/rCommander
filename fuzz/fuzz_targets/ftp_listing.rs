//! FTP `LIST` (Unix and DOS formats) and `MLSD` answers (`rc_remote::ftp::parse_listing`), read
//! both ways and in both encodings: whatever a server sends, every entry that comes out names
//! one entry of the listed folder (`listed_name::safe_name`, RC-060-05).

#![no_main]

use std::time::{Duration, SystemTime};

use libfuzzer_sys::fuzz_target;
use rc_remote::ftp::{NameEncoding, parse_listing};

fuzz_target!(|data: &[u8]| {
    // A fixed "now": the year a `Mon DD HH:MM` stamp gets must not depend on the day of the run.
    let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1_792_238_400);
    for mlsd in [false, true] {
        for fallback in [NameEncoding::Latin1, NameEncoding::Utf8] {
            for entry in parse_listing(data, mlsd, fallback, now) {
                let name = entry.name.as_str();
                assert!(
                    !matches!(name, "" | "." | "..")
                        && !name.contains(['/', '\\'])
                        && !name.chars().any(char::is_control)
                        && rc_vfs::drive_letter(name).is_none(),
                    "a listing gave the unsafe name {name:?}"
                );
            }
        }
    }
});
