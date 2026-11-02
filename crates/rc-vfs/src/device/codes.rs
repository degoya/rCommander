//! What the WPD calls answer, read without Windows: the error codes, the dates and the device ids
//! that are no MTP device. Portable, so the tests run everywhere; only `windows.rs` calls it.

#![cfg_attr(
    not(windows),
    allow(dead_code, reason = "only the WPD session reads these")
)]

use std::{io, time::SystemTime};

use chrono::{Duration, Local, NaiveDate, NaiveDateTime, TimeZone};

use super::DeviceError;

/// The `HRESULT`s of a device that is gone, hangs or keeps its files to itself: Win32's
/// `ERROR_DEVICE_NOT_CONNECTED`, `ERROR_NO_SUCH_DEVICE`, `ERROR_NOT_READY`, `ERROR_GEN_FAILURE`
/// (what an unplugged phone answers), `ERROR_BUSY`, `ERROR_SEM_TIMEOUT`, and WPD's
/// `E_WPD_DEVICE_NOT_OPEN` and `E_WPD_DEVICE_IS_HUNG`.
const UNAVAILABLE: &[u32] = &[
    0x8007_048F,
    0x8007_01B1,
    0x8007_0015,
    0x8007_001F,
    0x8007_00AA,
    0x8007_0079,
    0x802A_0002,
    0x802A_0006,
];

/// `ERROR_FILE_NOT_FOUND`, `ERROR_PATH_NOT_FOUND`, `ERROR_NOT_FOUND`.
const NOT_FOUND: &[u32] = &[0x8007_0002, 0x8007_0003, 0x8007_0490];

/// `ERROR_FILE_EXISTS`, `ERROR_ALREADY_EXISTS`.
const EXISTS: &[u32] = &[0x8007_0050, 0x8007_00B7];

/// `E_ACCESSDENIED`.
const DENIED: u32 = 0x8007_0005;

/// `E_NOTIMPL`, `ERROR_NOT_SUPPORTED`.
const UNSUPPORTED: &[u32] = &[0x8000_4001, 0x8007_0032];

/// `ERROR_CANCELLED`.
const CANCELLED: u32 = 0x8007_04C7;

/// The failure a WPD call's `HRESULT` names.
pub(crate) fn failure(code: u32) -> DeviceError {
    if UNAVAILABLE.contains(&code) {
        DeviceError::Unavailable
    } else if NOT_FOUND.contains(&code) {
        DeviceError::NotFound
    } else if EXISTS.contains(&code) {
        DeviceError::Exists
    } else if code == DENIED {
        DeviceError::Denied
    } else if UNSUPPORTED.contains(&code) {
        DeviceError::Unsupported("this operation on the device")
    } else if code == CANCELLED {
        DeviceError::Cancelled
    } else {
        DeviceError::Io(io::Error::other(format!(
            "the device answered 0x{code:08X}"
        )))
    }
}

/// An OLE automation date (`VT_DATE`: days since 1899-12-30, the fraction the time of day) as
/// a date and time without a zone; `None` for one out of range.
pub(crate) fn ole_date(days: f64) -> Option<NaiveDateTime> {
    if !days.is_finite() {
        return None;
    }
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30)?.and_hms_opt(0, 0, 0)?;
    // Whole milliseconds are more than a device keeps; the cast saturates outside i64.
    let millis = (days * 86_400_000.0).round() as i64;
    epoch.checked_add_signed(Duration::try_milliseconds(millis)?)
}

/// A device's date (local time, as WPD hands it out) as a point in time.
pub(crate) fn local_time(date: NaiveDateTime) -> Option<SystemTime> {
    Local
        .from_local_datetime(&date)
        .earliest()
        .map(SystemTime::from)
}

/// Whether the WPD device `id` is a drive Windows mounts with a letter of its own (a USB stick,
/// a card reader): WPD lists those too, the drive bar has them already.
pub(crate) fn is_mass_storage(id: &str) -> bool {
    let id = id.to_ascii_lowercase();
    id.contains("wpdbusenum") || id.contains("usbstor")
}

#[cfg(test)]
#[path = "codes_tests.rs"]
mod tests;
