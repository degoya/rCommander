//! The user's short date and time format (`platform/locale_format.rs`): `GetLocaleInfoEx`, which
//! neither std nor winsafe wraps (owner, 2026-11-07, RC-100-96).

#![allow(
    unsafe_code,
    reason = "GetLocaleInfoEx is an unsafe fn of windows-sys without a safe wrapper in std or \
              winsafe; the one block says why it holds"
)]

use windows_sys::Win32::Globalization::{GetLocaleInfoEx, LOCALE_SSHORTDATE, LOCALE_STIMEFORMAT};

/// The buffer in UTF-16 units: Windows keeps both formats within 80 characters.
const CAPACITY: usize = 128;

/// The user's locale information `kind`, `None` when the call fails or gives nothing.
fn info(kind: u32) -> Option<String> {
    let mut buffer = [0_u16; CAPACITY];
    let size = i32::try_from(buffer.len()).ok()?;
    // SAFETY: a null locale name is LOCALE_NAME_USER_DEFAULT; `buffer` is writable for `size`
    // UTF-16 units, the size passed, and the call writes at most that many, its terminating
    // null included. It returns the units written, or 0 on failure.
    let written = unsafe { GetLocaleInfoEx(std::ptr::null(), kind, buffer.as_mut_ptr(), size) };
    let len = usize::try_from(written).ok()?.checked_sub(1)?;
    String::from_utf16(buffer.get(..len)?)
        .ok()
        .filter(|format| !format.is_empty())
}

/// The short date and the time format of the user's locale.
pub fn formats() -> (Option<String>, Option<String>) {
    (info(LOCALE_SSHORTDATE), info(LOCALE_STIMEFORMAT))
}
