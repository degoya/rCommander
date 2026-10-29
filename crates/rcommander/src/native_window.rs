//! The Win32 handle (`HWND`) of an iced window, for the platform modules that need one: drag
//! out to Explorer (RC-080-01), the shell context menu (RC-080-02) and the file dialog
//! (RC-010-18).
//!
//! iced 0.14 hands it out without a dependency of its own: `iced::window::run` calls a closure
//! with the live window, which implements `raw_window_handle::HasWindowHandle` (iced re-exports
//! the crate as `iced::window::raw_window_handle`). The closure runs on the event loop's
//! thread, where Win32 wants the handle used. ADR 0001 records the finding. [`on_window`] runs a
//! platform call there, [`raw_hwnd`] turns the number into the handle the Win32 calls take - one
//! place for both (RC-080-53).

use std::num::NonZeroIsize;

use iced::{
    Task,
    window::{self, raw_window_handle::RawWindowHandle},
};

/// A task that yields the `HWND` of the window `id`, or `None` when it has no Win32 handle. A
/// window that closes before the task runs yields nothing. The owner of the file dialog
/// (RC-010-18) and the colour dialog, which run on a thread of their own.
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "the dialogs, its callers, are Windows only")
)]
pub fn hwnd(id: window::Id) -> Task<Option<NonZeroIsize>> {
    window::run(id, hwnd_of)
}

/// The `HWND` of the live `window` that `window::run` hands out, or `None` when it has no Win32
/// handle; for a platform module that uses the handle right there, on the window's thread (the
/// shell context menu).
pub fn hwnd_of(window: &dyn window::Window) -> Option<NonZeroIsize> {
    match window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd),
        _ => None,
    }
}

/// The window handle the number from [`hwnd`] or [`hwnd_of`] stands for, as the Win32 calls take
/// it (`HWND`, a pointer).
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "the Windows platform modules are its callers")
)]
pub fn raw_hwnd(handle: NonZeroIsize) -> *mut std::ffi::c_void {
    std::ptr::with_exposed_provenance_mut(handle.get() as usize)
}

/// Runs `work` on the app's main window - the oldest - on the window's thread; `fallback`'s
/// answer when there is none.
#[cfg_attr(
    not(windows),
    expect(dead_code, reason = "the Windows platform modules are its callers")
)]
pub fn on_window<T: Send + 'static>(
    work: impl Fn(&dyn window::Window) -> T + Clone + Send + 'static,
    fallback: fn() -> T,
) -> Task<T> {
    window::oldest().then(move |id| match id {
        Some(id) => window::run(id, work.clone()),
        None => Task::done(fallback()),
    })
}
