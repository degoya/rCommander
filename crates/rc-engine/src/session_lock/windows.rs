//! Windows: the session lock and suspend notices, received by a hidden window on a thread of its
//! own (RC-080-11).
//!
//! `WTSRegisterSessionNotification` sends `WM_WTSSESSION_CHANGE` to a window, and
//! `WM_POWERBROADCAST` goes to top-level windows only - a message-only window would never see it.
//! So the window is top-level, never shown, without a task-bar button (`WS_EX_TOOLWINDOW`), and
//! it and its message loop live for the rest of the process. The window procedure turns the two
//! notices into [`SessionEvent`]s ([`super::classify`]) and passes every message on to
//! `DefWindowProcW`.
//!
//! `unsafe` is allowed in this module, and only here: `windows-sys` offers the window class, the
//! window, the message loop and the WTS registration as bare `extern` functions, and the window
//! procedure is an `unsafe extern "system"` callback. Every call has its `SAFETY:` note.

#![allow(
    unsafe_code,
    reason = "the session and power notices reach only a Win32 window: its class, the window, \
              its message loop and the WTS registration are unsafe fns of windows-sys; every \
              block says why it holds"
)]

use std::{
    cell::RefCell,
    ptr::{null, null_mut},
};

use tokio::sync::broadcast;
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::{
        LibraryLoader::GetModuleHandleW,
        RemoteDesktop::{NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification},
    },
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, MSG, PBT_APMSUSPEND,
        RegisterClassExW, TranslateMessage, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSEXW,
        WS_EX_TOOLWINDOW, WS_OVERLAPPED, WTS_SESSION_LOCK,
    },
};

use super::SessionEvent;

// The portable numbers `classify` matches are Windows' own.
const _: () = assert!(
    super::WM_WTSSESSION_CHANGE == WM_WTSSESSION_CHANGE
        && super::WTS_SESSION_LOCK == WTS_SESSION_LOCK as usize
        && super::WM_POWERBROADCAST == WM_POWERBROADCAST
        && super::PBT_APMSUSPEND == PBT_APMSUSPEND as usize
);

thread_local! {
    /// Where the window procedure sends what it hears; set on the watching thread only.
    static SENDER: RefCell<Option<broadcast::Sender<SessionEvent>>> = const { RefCell::new(None) };
}

/// Starts watching on a thread of its own. A failure is logged: the vault then locks by its
/// idle lock only, as before.
pub(super) fn watch(sender: broadcast::Sender<SessionEvent>) {
    let started = std::thread::Builder::new()
        .name("rc-session-lock".to_owned())
        .spawn(move || run(sender));
    if let Err(error) = started {
        tracing::warn!(%error, "the session lock cannot be watched");
    }
}

/// The hidden window and its message loop; returns only when either cannot be had.
fn run(sender: broadcast::Sender<SessionEvent>) {
    SENDER.with(|cell| *cell.borrow_mut() = Some(sender));
    let class_name = wide("rCommanderSessionLock");
    // SAFETY: a null name asks for the module of the running executable; nothing is freed.
    let instance = unsafe { GetModuleHandleW(null()) };
    let class = WNDCLASSEXW {
        cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
        lpfnWndProc: Some(procedure),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..WNDCLASSEXW::default()
    };
    // SAFETY: `class` is fully initialized and its name a NUL-terminated buffer that outlives the
    // call; Windows copies both.
    if unsafe { RegisterClassExW(&class) } == 0 {
        tracing::warn!("the session lock's window class cannot be registered");
        return;
    }
    // SAFETY: the class was registered above with this instance; no parent, no menu, no
    // creation data. The window is never shown.
    let window = unsafe {
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            null(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            null_mut(),
            null_mut(),
            instance,
            null(),
        )
    };
    if window.is_null() {
        tracing::warn!("the session lock's window cannot be created");
        return;
    }
    // SAFETY: `window` is the live window of this thread. Without the registration (Terminal
    // Services not running) the power notices still arrive.
    if unsafe { WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION) } == 0 {
        tracing::warn!("the session lock cannot be watched; suspend still locks the vault");
    }
    let mut message = MSG::default();
    // SAFETY: `message` is ours for each call; the loop ends on WM_QUIT (0) or an error (-1).
    while unsafe { GetMessageW(&mut message, null_mut(), 0, 0) } > 0 {
        // SAFETY: `message` was filled by GetMessageW just now, on this thread.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// The window procedure: a session notice to the listeners, every message on to Windows.
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if let Some(event) = super::classify(message, wparam) {
        SENDER.with(|cell| {
            if let Some(sender) = cell.borrow().as_ref() {
                // No listener is not an error: no engine has a vault yet.
                let _ = sender.send(event);
            }
        });
    }
    // SAFETY: the message passes on unchanged to the default procedure, as a window must.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// `text` as a NUL-terminated UTF-16 buffer.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
