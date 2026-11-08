use super::*;

fn pack(year: u32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> i32 {
    (((year - 1980) << 25)
        | (month << 21)
        | (day << 16)
        | (hour << 11)
        | (minute << 5)
        | (second / 2)) as i32
}

#[test]
fn dos_times_become_wall_clock_seconds() {
    assert_eq!(dos_time(pack(1980, 1, 1, 0, 0, 0)), Some(315_532_800));
    // 2024-02-29 13:45:58, a leap day.
    assert_eq!(dos_time(pack(2024, 2, 29, 13, 45, 58)), Some(1_709_214_358));
    // The last DOS year, which sets the sign bit of the `int`.
    assert_eq!(
        dos_time(pack(2107, 12, 31, 23, 59, 58)),
        Some(4_354_819_198)
    );
}

#[test]
fn dos_times_out_of_range_are_none() {
    assert_eq!(dos_time(0), None);
    assert_eq!(dos_time(pack(2000, 13, 1, 0, 0, 0)), None);
    assert_eq!(dos_time(pack(2000, 1, 1, 24, 0, 0)), None);
    assert_eq!(dos_time(pack(2000, 1, 1, 0, 60, 0)), None);
    assert_eq!(dos_time(pack(2000, 1, 1, 0, 0, 0) | 0x1e), None);
}

#[test]
fn sizes_join_their_halves() {
    assert_eq!(size(5, 0), 5);
    assert_eq!(size(0, 1), 1 << 32);
    assert_eq!(size(u32::MAX, u32::MAX), u64::MAX);
}

#[test]
fn error_codes_have_their_names() {
    assert_eq!(error_name(13), "E_BAD_ARCHIVE");
    assert_eq!(error_name(24), "E_NOT_SUPPORTED");
    assert_eq!(error_name(99), "error 99");
}

#[test]
fn entry_names_stay_below_the_folder() {
    for good in ["a.txt", "dir\\b.txt", "dir/sub/c", "..hidden", "a..b"] {
        assert!(safe_entry_name(good), "{good}");
    }
    let long = "x".repeat(MAX_NAME + 1);
    for bad in [
        "",
        "..\\evil",
        "dir\\..\\..\\evil",
        "a/../b",
        ".\\a",
        "\\abs",
        "/abs",
        "C:\\abs",
        "C:rel",
        "file:stream",
        "nul\0byte",
        long.as_str(),
    ] {
        assert!(!safe_entry_name(bad), "{bad}");
    }
}
