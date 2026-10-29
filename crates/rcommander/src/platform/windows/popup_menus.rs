//! The Win32 half of the dark popup menus (RC-080-18): uxtheme's undocumented
//! `SetPreferredAppMode` (ordinal 135) and `FlushMenuThemes` (ordinal 136), looked up by ordinal
//! once. On a build older than 1903, without uxtheme or without either ordinal nothing is called
//! and the menus stay light (`platform/popup_menus.rs` has the rule and its reasons).

#![allow(
    unsafe_code,
    reason = "uxtheme's menu mode exists only as undocumented exports, reached through \
              LoadLibraryExW, GetProcAddress by ordinal and a cast of the address to the \
              function's signature; every block says why it holds"
)]

use std::{ptr, sync::OnceLock};

use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};

use crate::platform::{
    menu_bar::BarPaint,
    popup_menus::{build_number, mode, supported},
};

const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
const SET_PREFERRED_APP_MODE: usize = 135;
const FLUSH_MENU_THEMES: usize = 136;

/// `PreferredAppMode SetPreferredAppMode(PreferredAppMode)`.
type SetPreferredAppMode = unsafe extern "system" fn(i32) -> i32;
/// `void FlushMenuThemes()`.
type FlushMenuThemes = unsafe extern "system" fn();

#[derive(Clone, Copy)]
struct Uxtheme {
    set_mode: SetPreferredAppMode,
    flush: FlushMenuThemes,
}

/// The two functions, looked up once; `None` where they cannot be trusted or found.
static UXTHEME: OnceLock<Option<Uxtheme>> = OnceLock::new();

/// Asks Windows for dark or light popup menus and redraws its menu themes; silently nothing
/// where that is not possible, or where `RC_MENU_BAR_THEME=light` leaves the menus alone.
pub fn follow(dark: bool) {
    if !BarPaint::get().popups() {
        return;
    }
    let Some(uxtheme) = *UXTHEME.get_or_init(load) else {
        return;
    };
    // SAFETY: both addresses are uxtheme's exports 135 and 136 on a build where they are
    // `SetPreferredAppMode(PreferredAppMode) -> PreferredAppMode` and `FlushMenuThemes()`; the
    // library stays loaded for the process's life. The previous mode is not needed.
    unsafe {
        (uxtheme.set_mode)(mode(dark) as i32);
        (uxtheme.flush)();
    }
}

fn load() -> Option<Uxtheme> {
    if !supported(windows_build()) {
        return None;
    }
    let name: Vec<u16> = "uxtheme.dll".encode_utf16().chain([0]).collect();
    // SAFETY: a NUL-terminated name; only System32 is searched, so no other uxtheme.dll is
    // loaded. The handle is never freed: the functions are called for the process's life.
    let library =
        unsafe { LoadLibraryExW(name.as_ptr(), ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
    if library.is_null() {
        return None;
    }
    // SAFETY: a live module handle; an ordinal below 0x10000 in place of a name is what
    // `GetProcAddress` takes (MAKEINTRESOURCEA).
    let (set_mode, flush) = unsafe {
        (
            GetProcAddress(library, ptr::without_provenance(SET_PREFERRED_APP_MODE))?,
            GetProcAddress(library, ptr::without_provenance(FLUSH_MENU_THEMES))?,
        )
    };
    // SAFETY: the exports' signatures on the builds `supported` lets through (see `follow`);
    // a function pointer to a function pointer of the same size.
    Some(unsafe {
        Uxtheme {
            set_mode: std::mem::transmute::<
                unsafe extern "system" fn() -> isize,
                SetPreferredAppMode,
            >(set_mode),
            flush: std::mem::transmute::<unsafe extern "system" fn() -> isize, FlushMenuThemes>(
                flush,
            ),
        }
    })
}

/// The Windows build number, from the registry (`GetVersionEx` reports what the manifest
/// allows, not the build).
fn windows_build() -> Option<u32> {
    let key = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey(CURRENT_VERSION)
        .ok()?;
    let text: String = key.get_value("CurrentBuildNumber").ok()?;
    build_number(&text)
}
