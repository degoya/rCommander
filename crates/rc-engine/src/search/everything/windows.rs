//! Windows: Everything's IPC as its SDK 2 (`everything_ipc.h`, Everything 1.4) describes it.
//!
//! Everything is found by its IPC window's class (`EVERYTHING_IPC_WNDCLASS`; 1.5's default
//! instance adds `_(1.5a)`) and asked its version with `EVERYTHING_WM_IPC`. A query is an
//! `EVERYTHING_IPC_QUERY2` sent as `WM_COPYDATA`; Everything answers with a `WM_COPYDATA` to the
//! window the query names: a message-only window (`HWND_MESSAGE`) made for the query on the
//! calling thread - a blocking one, never the window's - whose loop runs until the answer came,
//! the stop switch was set or [`TIMEOUT`] passed, and which is destroyed then, so an answer that
//! comes later finds no one. The answer is copied as it arrives (at most [`MAX_REPLY_BYTES`]) and
//! read by the portable parser.
//!
//! `unsafe` is allowed in this module, and only here: `windows-sys` offers the window, its class,
//! its message loop and `SendMessageTimeoutW` as bare `extern` functions, the window procedure is
//! an `unsafe extern "system"` callback, and `WM_COPYDATA` hands over a raw pointer. Every block
//! has its `SAFETY:` note.

#![allow(
    unsafe_code,
    reason = "Everything answers only a Win32 window through WM_COPYDATA: the window, its class \
              and message loop, the sends and the copied block are unsafe fns and raw pointers \
              of windows-sys; every block says why it holds"
)]

use std::{
    cell::RefCell,
    ptr::{null, null_mut},
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    time::Instant,
};

use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    System::{DataExchange::COPYDATASTRUCT, LibraryLoader::GetModuleHandleW},
    UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW,
        HWND_MESSAGE, MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT,
        RegisterClassExW, SMTO_ABORTIFHUNG, SendMessageTimeoutW, TranslateMessage, WM_COPYDATA,
        WM_USER, WNDCLASSEXW,
    },
};

use super::{COPYDATA_QUERY2W, Everything, EverythingError, MAX_REPLY_BYTES, TIMEOUT, request};
use crate::search::SearchCancel;

/// The IPC window classes: Everything 1.4, and 1.5's default instance.
const CLASSES: [&str; 2] = [
    "EVERYTHING_TASKBAR_NOTIFICATION",
    "EVERYTHING_TASKBAR_NOTIFICATION_(1.5a)",
];
/// `EVERYTHING_WM_IPC`, with `EVERYTHING_IPC_GET_MAJOR_VERSION` and `_MINOR_VERSION`.
const WM_IPC: u32 = WM_USER;
const GET_MAJOR_VERSION: WPARAM = 0;
const GET_MINOR_VERSION: WPARAM = 1;
/// How long one message to Everything may wait, in milliseconds.
const SEND_TIMEOUT_MS: u32 = 1000;
/// How often the wait for the answer looks at the stop switch, in milliseconds.
const POLL_MS: u32 = 50;
/// The class of the window the answers come to.
const REPLY_CLASS: &str = "rCommanderEverythingReply";

/// What a query on this thread awaits: its id, and the answer once it came.
type Awaited = Option<(usize, Option<Result<Vec<u8>, EverythingError>>)>;

thread_local! {
    /// Set by a query, filled by the window procedure, on the querying thread only.
    static AWAITED: RefCell<Awaited> = const { RefCell::new(None) };
}

/// Numbers the queries, so an answer to an earlier one is not taken.
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// The Everything of this machine.
pub(super) struct System;

impl Everything for System {
    fn query(
        &self,
        search: &str,
        max: u32,
        cancel: &SearchCancel,
    ) -> Result<Vec<u8>, EverythingError> {
        let target = target().ok_or(EverythingError::NotRunning)?;
        let reply = ReplyWindow::new()?;
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        // Window handles carry 32 significant bits on 64-bit Windows too; the query has 32.
        let handle = reply.0 as usize as u32;
        let bytes = request(handle, id, search, max);
        let size = u32::try_from(bytes.len())
            .map_err(|_| EverythingError::System("the search is too long"))?;
        let data = COPYDATASTRUCT {
            dwData: COPYDATA_QUERY2W,
            cbData: size,
            lpData: bytes.as_ptr().cast_mut().cast(),
        };
        AWAITED.with(|cell| *cell.borrow_mut() = Some((id as usize, None)));
        let mut accepted = 0_usize;
        // SAFETY: `data` and the bytes it points at live until the call returns; WM_COPYDATA
        // copies them into Everything's process and Everything does not write them. `accepted`
        // is ours. The wait lets this thread take sent messages (the answer may come at once)
        // and ends after SEND_TIMEOUT_MS, at once for a hung Everything.
        let sent = unsafe {
            SendMessageTimeoutW(
                target,
                WM_COPYDATA,
                reply.0 as WPARAM,
                std::ptr::from_ref(&data) as LPARAM,
                SMTO_ABORTIFHUNG,
                SEND_TIMEOUT_MS,
                &mut accepted,
            )
        };
        if sent == 0 {
            return Err(EverythingError::TimedOut);
        }
        if accepted == 0 {
            return Err(EverythingError::System("Everything refused the query"));
        }
        let deadline = Instant::now() + TIMEOUT;
        loop {
            if let Some(answer) = AWAITED.with(|cell| {
                cell.borrow_mut()
                    .as_mut()
                    .and_then(|(_, answer)| answer.take())
            }) {
                return answer;
            }
            if cancel.is_cancelled() {
                return Err(EverythingError::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(EverythingError::TimedOut);
            }
            // SAFETY: no handles to wait on, only this thread's queue; returns after POLL_MS at
            // most.
            unsafe { MsgWaitForMultipleObjects(0, null(), 0, POLL_MS, QS_ALLINPUT) };
            pump();
        }
    }
}

/// The version of the running Everything, `(major, minor)`.
pub(super) fn version() -> Option<(u32, u32)> {
    let window = target()?;
    let major = send(window, WM_IPC, GET_MAJOR_VERSION)?;
    let minor = send(window, WM_IPC, GET_MINOR_VERSION)?;
    Some((u32::try_from(major).ok()?, u32::try_from(minor).ok()?))
}

/// Everything's IPC window, if Everything runs.
fn target() -> Option<HWND> {
    CLASSES.iter().find_map(|class| {
        let class = wide(class);
        // SAFETY: the class name is a NUL-terminated buffer that outlives the call; no title.
        let window = unsafe { FindWindowW(class.as_ptr(), null()) };
        (!window.is_null()).then_some(window)
    })
}

/// `message` with `wparam` (no pointer) sent to `window`: its answer, `None` when it failed or
/// timed out.
fn send(window: HWND, message: u32, wparam: WPARAM) -> Option<usize> {
    let mut result = 0_usize;
    // SAFETY: a stale `window` only fails the call; neither parameter is a pointer, `result` is
    // ours. The wait ends after SEND_TIMEOUT_MS, at once for a hung window.
    let sent = unsafe {
        SendMessageTimeoutW(
            window,
            message,
            wparam,
            0,
            SMTO_ABORTIFHUNG,
            SEND_TIMEOUT_MS,
            &mut result,
        )
    };
    (sent != 0).then_some(result)
}

/// Every message waiting for this thread, handed to its window.
fn pump() {
    let mut message = MSG::default();
    // SAFETY: `message` is ours; PM_REMOVE takes each message of this thread's queue once (sent
    // messages are delivered inside the call).
    while unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) } != 0 {
        // SAFETY: `message` was filled by PeekMessageW just now, on this thread.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

/// The message-only window an answer comes to, destroyed with the query.
struct ReplyWindow(HWND);

impl ReplyWindow {
    fn new() -> Result<Self, EverythingError> {
        if !registered() {
            return Err(EverythingError::System("the answer's window class"));
        }
        let class_name = wide(REPLY_CLASS);
        // SAFETY: a null name asks for the module of the running executable; nothing is freed.
        let instance = unsafe { GetModuleHandleW(null()) };
        // SAFETY: the class was registered with this instance; a message-only window
        // (HWND_MESSAGE), never shown, without menu or creation data.
        let window = unsafe {
            CreateWindowExW(
                0,
                class_name.as_ptr(),
                null(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                null_mut(),
                instance,
                null(),
            )
        };
        if window.is_null() {
            Err(EverythingError::System("the answer's window"))
        } else {
            Ok(Self(window))
        }
    }
}

impl Drop for ReplyWindow {
    fn drop(&mut self) {
        AWAITED.with(|cell| *cell.borrow_mut() = None);
        // SAFETY: the window was made on this thread and is destroyed once, here.
        unsafe { DestroyWindow(self.0) };
    }
}

/// Registers the answer window's class once per process; whether it is registered.
fn registered() -> bool {
    static REGISTERED: OnceLock<bool> = OnceLock::new();
    *REGISTERED.get_or_init(|| {
        let class_name = wide(REPLY_CLASS);
        // SAFETY: a null name asks for the module of the running executable; nothing is freed.
        let instance = unsafe { GetModuleHandleW(null()) };
        let class = WNDCLASSEXW {
            cbSize: u32::try_from(size_of::<WNDCLASSEXW>()).unwrap_or(u32::MAX),
            lpfnWndProc: Some(procedure),
            hInstance: instance,
            lpszClassName: class_name.as_ptr(),
            ..WNDCLASSEXW::default()
        };
        // SAFETY: `class` is fully initialized and its name a NUL-terminated buffer that outlives
        // the call; Windows copies both.
        unsafe { RegisterClassExW(&class) != 0 }
    })
}

/// The window procedure: the awaited answer copied, every other message on to Windows.
unsafe extern "system" fn procedure(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_COPYDATA && lparam != 0 {
        // SAFETY: for WM_COPYDATA `lparam` points at the COPYDATASTRUCT Windows marshalled the
        // sender's block into, valid while this call runs.
        let data = unsafe { &*(lparam as *const COPYDATASTRUCT) };
        let taken = AWAITED.with(|cell| match cell.borrow_mut().as_mut() {
            Some((id, slot)) if *id == data.dwData && slot.is_none() => {
                let size = data.cbData as usize;
                *slot = Some(if size > MAX_REPLY_BYTES {
                    Err(EverythingError::Reply("too large"))
                } else if data.lpData.is_null() || size == 0 {
                    Ok(Vec::new())
                } else {
                    // SAFETY: `lpData` points at `cbData` readable bytes while this call runs
                    // (WM_COPYDATA's contract); they are copied at once.
                    let bytes =
                        unsafe { std::slice::from_raw_parts(data.lpData.cast::<u8>(), size) };
                    Ok(bytes.to_vec())
                });
                true
            }
            _ => false,
        });
        if taken {
            return 1;
        }
    }
    // SAFETY: the message passes on unchanged to the default procedure, as a window must.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

/// `text` as a NUL-terminated UTF-16 buffer.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}
