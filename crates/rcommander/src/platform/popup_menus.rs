//! Dark popup menus (RC-080-18, owner's decision 2026-10-18): the menus that drop from the menu
//! bar and the Explorer's context menu are drawn by Windows, which draws them dark only for a
//! program that asks through uxtheme's undocumented `SetPreferredAppMode` (ordinal 135) and then
//! `FlushMenuThemes` (ordinal 136) - as Explorer, Notepad++ and Windows Terminal do. They follow
//! the app's look, "System" included, and change with it.
//!
//! Undocumented means a Windows update may move or drop the ordinals: then the menus stay light,
//! nothing fails. Ordinal 135 is `SetPreferredAppMode` from Windows 10 1903 (build 18362); before
//! it, the same ordinal was `AllowDarkModeForApp(BOOL)`, so older builds are left alone
//! ([`supported`]). The calls are `windows/popup_menus.rs`; elsewhere this does nothing.

use iced::{Task, window};

/// The first build where ordinal 135 is `SetPreferredAppMode` (Windows 10 1903).
pub const FIRST_BUILD: u32 = 18362;

/// uxtheme's `PreferredAppMode`, the two values rCommander sets.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum AppMode {
    ForceDark = 2,
    ForceLight = 3,
}

/// The mode for the app's look: dark menus with the dark look, light ones with the light look -
/// whichever the theme setting chose, the system's included.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
pub fn mode(dark: bool) -> AppMode {
    if dark {
        AppMode::ForceDark
    } else {
        AppMode::ForceLight
    }
}

/// Whether the Windows build `build` (`None`: unknown) has ordinal 135 as `SetPreferredAppMode`.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
pub fn supported(build: Option<u32>) -> bool {
    build.is_some_and(|build| build >= FIRST_BUILD)
}

/// The build number from the registry's `CurrentBuildNumber` text.
#[cfg_attr(
    not(windows),
    allow(dead_code, reason = "read by the Win32 half and the tests")
)]
pub fn build_number(text: &str) -> Option<u32> {
    text.trim().parse().ok()
}

/// Draws the popup menus dark or light as `dark` says; on the thread of the main window `id`.
#[cfg(windows)]
pub fn follow(id: window::Id, dark: bool) -> Task<()> {
    window::run(id, move |_| super::windows::popup_menus::follow(dark))
}

/// Draws the popup menus as `dark` says: nothing to do outside Windows.
#[cfg(not(windows))]
pub fn follow(_id: window::Id, _dark: bool) -> Task<()> {
    Task::none()
}

#[cfg(test)]
#[path = "popup_menus_tests.rs"]
mod tests;
