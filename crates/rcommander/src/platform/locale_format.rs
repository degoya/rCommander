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
