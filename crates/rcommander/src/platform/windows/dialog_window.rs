//! The Win32 half of a dialog's window of its own (RC-080-15): iced 0.14 has no owner window,
//! so the dialog is given one here - the main window - while it is still invisible, together with
//! its own Alt+Tab entry, its place on the monitors and, for a modal dialog, the main window's
//! lock; [`release`] unlocks the main window before such a dialog closes. A modeless one (the
//! queue, RC-010-14) leaves the main window enabled.

#![allow(
    unsafe_code,
    reason = "an owner window, the extended style, the monitors' work areas and the lock of the \
              main window exist only as Win32 calls, all unsafe fns of windows-sys; every block \
              says why it holds"
)]

use std::{num::NonZeroIsize, ptr};

use iced::window;
use windows_sys::{
    Win32::{
        Foundation::{FALSE, HWND, LPARAM, RECT, TRUE},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
        UI::{
            Input::KeyboardAndMouse::EnableWindow,
            WindowsAndMessaging::{
                GWL_EXSTYLE, GWLP_HWNDPARENT, GetWindowLongPtrW, GetWindowRect,
                MONITORINFOF_PRIMARY, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowLongPtrW,
                SetWindowPos, WS_EX_APPWINDOW,
            },
        },
    },
    core::BOOL,
};

use crate::{
    native_window::raw_hwnd as hwnd,
    platform::dialog_window::{Rect, clamp},
};

/// Makes the live `dialog` window, not yet shown, the dialog of the main window `owner`: owned by
/// it, with its own Alt+Tab and taskbar entry, on a monitor, and - `modal` - the main window
/// locked while it is open. Without an owner it only gets its entry and its place.
pub fn own(dialog: &dyn window::Window, owner: Option<NonZeroIsize>, modal: bool) {
    let Some(dialog) = crate::native_window::hwnd_of(dialog).map(hwnd) else {
        return;
    };
    // SAFETY: `dialog` is the live window `window::run` handed out, on its own thread; the
    // owner is the live main window, which outlives its dialogs. Changing the owner and the
    // extended style of a window that is not shown yet is what both calls are for.
    unsafe {
        if let Some(owner) = owner {
            SetWindowLongPtrW(dialog, GWLP_HWNDPARENT, owner.get());
        }
        let style = GetWindowLongPtrW(dialog, GWL_EXSTYLE);
        SetWindowLongPtrW(dialog, GWL_EXSTYLE, style | WS_EX_APPWINDOW as isize);
    }
    place(dialog);
    if let Some(owner) = owner.filter(|_| modal) {
        // SAFETY: the live main window; a disabled window takes no input until `release`.
        unsafe { EnableWindow(hwnd(owner), FALSE) };
    }
}

/// The main window takes input again; before its dialog closes, so Windows gives it the focus.
pub fn release(main: &dyn window::Window) {
    if let Some(main) = crate::native_window::hwnd_of(main) {
        // SAFETY: the live main window `window::run` handed out, on its own thread.
        unsafe { EnableWindow(hwnd(main), TRUE) };
    }
}

/// Moves `dialog` onto the monitors' work areas when its title bar is on none of them.
fn place(dialog: HWND) {
    let mut frame = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    // SAFETY: a live window and a RECT the call writes.
    if unsafe { GetWindowRect(dialog, &mut frame) } == FALSE {
        return;
    }
    let window = rect(frame);
    let placed = clamp(window, &work_areas());
    if placed == window {
        return;
    }
    // SAFETY: a live window, not shown yet; its z-order and activation stay as they are.
    unsafe {
        SetWindowPos(
            dialog,
            ptr::null_mut(),
            placed.x,
            placed.y,
            placed.width,
            placed.height,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// The work areas of the monitors - the screen without the taskbar - the primary first.
fn work_areas() -> Vec<Rect> {
    let mut areas: Vec<(bool, Rect)> = Vec::new();
    // SAFETY: the callback runs inside the call, on this thread, while `areas` lives; it reads
    // the pointer back as the vector it is.
    unsafe {
        EnumDisplayMonitors(
            ptr::null_mut(),
            ptr::null(),
            Some(monitor),
            ptr::from_mut(&mut areas).expose_provenance() as LPARAM,
        );
    }
    areas.sort_by_key(|(primary, _)| !primary);
    areas.into_iter().map(|(_, area)| area).collect()
}

/// One monitor of [`work_areas`]: its work area and whether it is the primary one.
unsafe extern "system" fn monitor(
    monitor: HMONITOR,
    _hdc: HDC,
    _bounds: *mut RECT,
    areas: LPARAM,
) -> BOOL {
    let mut info = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        rcMonitor: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        rcWork: RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        },
        dwFlags: 0,
    };
    // SAFETY: the monitor the enumeration hands over and an info block of the size it says;
    // `areas` is the vector `work_areas` passed, alive for the whole enumeration.
    unsafe {
        if GetMonitorInfoW(monitor, &mut info) != FALSE {
            let areas = &mut *ptr::with_exposed_provenance_mut::<Vec<(bool, Rect)>>(areas as usize);
            areas.push((info.dwFlags & MONITORINFOF_PRIMARY != 0, rect(info.rcWork)));
        }
    }
    TRUE
}

fn rect(frame: RECT) -> Rect {
    Rect {
        x: frame.left,
        y: frame.top,
        width: frame.right - frame.left,
        height: frame.bottom - frame.top,
    }
}
