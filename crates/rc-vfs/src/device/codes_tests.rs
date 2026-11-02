use chrono::{NaiveDate, NaiveDateTime};

use super::{failure, is_mass_storage, ole_date};
use crate::device::DeviceError;

fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32) -> NaiveDateTime {
    NaiveDate::from_ymd_opt(year, month, day)
        .and_then(|date| date.and_hms_opt(hour, minute, 0))
        .expect("a valid date")
}

#[test]
fn an_unplugged_or_hung_device_is_unavailable() {
    for code in [0x8007_048F, 0x8007_001F, 0x802A_0006, 0x802A_0002] {
        assert!(
            matches!(failure(code), DeviceError::Unavailable),
            "0x{code:08X}"
        );
    }
}

#[test]
fn the_other_codes_keep_their_kind() {
    assert!(matches!(failure(0x8007_0002), DeviceError::NotFound));
    assert!(matches!(failure(0x8007_00B7), DeviceError::Exists));
    assert!(matches!(failure(0x8007_0005), DeviceError::Denied));
    assert!(matches!(failure(0x8000_4001), DeviceError::Unsupported(_)));
    assert!(matches!(failure(0x8007_04C7), DeviceError::Cancelled));
    match failure(0x8000_FFFF) {
        DeviceError::Io(error) => assert!(error.to_string().contains("0x8000FFFF")),
        other => panic!("an unknown code is an I/O error, not {other:?}"),
    }
}

#[test]
fn an_ole_date_counts_days_from_1899() {
    assert_eq!(ole_date(0.0), Some(at(1899, 12, 30, 0, 0)));
    assert_eq!(ole_date(25_569.0), Some(at(1970, 1, 1, 0, 0)));
    assert_eq!(ole_date(45_658.75), Some(at(2025, 1, 1, 18, 0)));
    assert_eq!(ole_date(f64::NAN), None);
    assert_eq!(ole_date(f64::INFINITY), None);
}

#[test]
fn drives_with_a_letter_are_no_portable_device() {
    assert!(is_mass_storage(
        r"\\?\SWD#WPDBUSENUM#_??_USBSTOR#DISK&VEN_SANDISK&PROD_ULTRA#{53f56307}"
    ));
    assert!(!is_mass_storage(
        r"\\?\usb#vid_18d1&pid_4ee1&mi_00#8a1b2c3d#{6ac27878-a6fa-4155-ba85-f98f491d4f33}"
    ));
}
