//! The local date and time formats of the multi-rename tool's `[d]` and `[t]` (RC-100-96): the
//! user's short date and time pattern on Windows (`windows/locale_format.rs`), as Total
//! Commander takes them from the country settings. Elsewhere there is none, and the engine
//! writes `yyyy-MM-dd` and `HH:mm:ss`.

/// The short date and the time format, as Windows pictures (`dd.MM.yyyy`, `HH:mm:ss`); `None`
/// where the system gives none.
pub fn formats() -> (Option<String>, Option<String>) {
    #[cfg(windows)]
    {
        super::windows::locale_format::formats()
    }
    #[cfg(not(windows))]
    {
        (None, None)
    }
}

/// The text `GetLocaleInfoEx` wrote into `buffer`: `written` UTF-16 units, its terminating NUL
/// included. `None` for a failed call (0), a count past the buffer, units that are no UTF-16,
/// or an empty text. Apart from the call, so it is tested on every platform (RC-110-70).
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Windows half and the tests")
)]
pub(super) fn written_text(buffer: &[u16], written: i32) -> Option<String> {
    let written = usize::try_from(written).ok()?;
    if written > buffer.len() {
        return None;
    }
    let len = written.checked_sub(1)?;
    String::from_utf16(buffer.get(..len)?)
        .ok()
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
#[path = "locale_format_tests.rs"]
mod tests;
