//! Windows' accessibility settings (RC-110-79): whether a screen reader runs and whether high
//! contrast is on (`SystemParametersInfoW`, which neither std nor winsafe wraps safely), the
//! high-contrast theme's colours (winsafe's `GetSysColor`) and the text size of Settings,
//! Accessibility (`TextScaleFactor` in the registry, as WinUI's `UISettings` reads it without
//! WinRT).

#![allow(
    unsafe_code,
    reason = "SystemParametersInfoW is an unsafe fn of windows-sys (and of winsafe) without a safe \
              wrapper; each of the two blocks says why it holds"
)]

use windows_sys::Win32::{
    Foundation::FALSE,
    UI::{
        Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW},
        WindowsAndMessaging::{SPI_GETHIGHCONTRAST, SPI_GETSCREENREADER, SystemParametersInfoW},
    },
};
use winreg::{RegKey, enums::HKEY_CURRENT_USER};
use winsafe::{GetSysColor, co::COLOR};

use super::SystemColours;

/// Where Settings, Accessibility, Text size keeps its value.
const ACCESSIBILITY: &str = r"Software\Microsoft\Accessibility";

/// Whether a screen reader says it runs.
pub(super) fn screen_reader() -> bool {
    let mut running = FALSE;
    // SAFETY: SPI_GETSCREENREADER writes one BOOL through `pvparam`, which points at `running`,
    // a BOOL alive and writable for the call; `uiparam` and `fwinini` are 0 as the action asks.
    let read =
        unsafe { SystemParametersInfoW(SPI_GETSCREENREADER, 0, (&raw mut running).cast(), 0) };
    read != FALSE && running != FALSE
}

/// The high-contrast theme's colours while high contrast is on; `None` while it is off or the
/// call fails.
pub(super) fn high_contrast() -> Option<SystemColours> {
    let size = u32::try_from(std::mem::size_of::<HIGHCONTRASTW>()).ok()?;
    let mut contrast = HIGHCONTRASTW {
        cbSize: size,
        dwFlags: 0,
        lpszDefaultScheme: std::ptr::null_mut(),
    };
    // SAFETY: SPI_GETHIGHCONTRAST fills the HIGHCONTRASTW `pvparam` points at, whose `cbSize`
    // and `uiparam` both carry its size; `contrast` is alive and writable for the call. The
    // scheme's name it may point at belongs to the system and is never read here.
    let read =
        unsafe { SystemParametersInfoW(SPI_GETHIGHCONTRAST, size, (&raw mut contrast).cast(), 0) };
    if read == FALSE || contrast.dwFlags & HCF_HIGHCONTRASTON == 0 {
        return None;
    }
    let rgb = |index: COLOR| {
        let colour = GetSysColor(index);
        (u32::from(colour.GetRValue()) << 16)
            | (u32::from(colour.GetGValue()) << 8)
            | u32::from(colour.GetBValue())
    };
    Some(SystemColours {
        window: rgb(COLOR::WINDOW),
        window_text: rgb(COLOR::WINDOWTEXT),
        highlight: rgb(COLOR::HIGHLIGHT),
        highlight_text: rgb(COLOR::HIGHLIGHTTEXT),
        hot_light: rgb(COLOR::HOTLIGHT),
        gray_text: rgb(COLOR::GRAYTEXT),
    })
}

/// The text size in percent; `None` where it was never changed (100 %).
pub(super) fn text_scale_percent() -> Option<u32> {
    let key = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey(ACCESSIBILITY)
        .ok()?;
    key.get_value::<u32, _>("TextScaleFactor").ok()
}
