//! Windows: a start through the shell, `ShellExecuteExW` (RC-100-86), with the verb `runas` for
//! a start as administrator.
//!
//! `unsafe` is allowed in this module, and only here: `windows-sys` offers `ShellExecuteExW` as a
//! bare `extern` function, and neither `std` nor `winsafe` (which this crate has with the
//! `kernel` feature only) has a safe wrapper for it. Every string handed over is a NUL-terminated
//! UTF-16 buffer owned by [`shell_execute`] and alive for the whole call; the call has its
//! `SAFETY:` note. It waits while Windows asks the person (UAC), so it runs on the blocking pool
//! only (`elevate.rs`).
#![allow(
    unsafe_code,
    reason = "ShellExecuteExW is a bare extern fn of windows-sys with no safe wrapper in std or \
              winsafe; the block has its SAFETY note"
)]

use std::{ffi::OsStr, mem::size_of, os::windows::ffi::OsStrExt, ptr::null};

use windows_sys::Win32::UI::Shell::{SHELLEXECUTEINFOW, ShellExecuteExW};

use super::{ShellStart, ShellStarted};
use crate::user_commands::Show;

/// `SEE_MASK_NOASYNC`: the call returns only once the start is done (or refused).
const SEE_MASK_NOASYNC: u32 = 0x0000_0100;

/// `SW_SHOWNORMAL`, `SW_SHOWMINNOACTIVE`, `SW_SHOWMAXIMIZED`.
const SW_SHOWNORMAL: i32 = 1;
const SW_SHOWMINNOACTIVE: i32 = 7;
const SW_SHOWMAXIMIZED: i32 = 3;

/// `ERROR_CANCELLED`: the person said no to Windows' question.
const ERROR_CANCELLED: i32 = 1223;

/// `text` as a NUL-terminated UTF-16 buffer.
fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(std::iter::once(0)).collect()
}

/// Starts `start` through the shell: `Ok(Cancelled)` when the person said no; the error is
/// Windows' code.
pub(super) fn shell_execute(start: &ShellStart) -> Result<ShellStarted, u32> {
    let verb = start.elevated.then(|| wide(OsStr::new("runas")));
    let file = wide(start.program.as_os_str());
    let params = wide(OsStr::new(&start.params));
    let folder = start.folder.as_ref().map(|folder| wide(folder.as_os_str()));
    let show = match start.show {
        Show::Normal => SW_SHOWNORMAL,
        Show::Minimized => SW_SHOWMINNOACTIVE,
        Show::Maximized => SW_SHOWMAXIMIZED,
    };
    let mut info = SHELLEXECUTEINFOW {
        cbSize: u32::try_from(size_of::<SHELLEXECUTEINFOW>())
            .expect("the struct's size fits in a u32"),
        fMask: SEE_MASK_NOASYNC,
        lpVerb: verb.as_ref().map_or(null(), |verb| verb.as_ptr()),
        lpFile: file.as_ptr(),
        lpParameters: params.as_ptr(),
        lpDirectory: folder.as_ref().map_or(null(), |folder| folder.as_ptr()),
        nShow: show,
        ..SHELLEXECUTEINFOW::default()
    };
    // SAFETY: `info` is a SHELLEXECUTEINFOW with its own size in `cbSize`; every pointer in it is
    // null or points at a NUL-terminated UTF-16 buffer of this function (`verb`, `file`,
    // `params`, `folder`) that outlives the call; no window handle is given. With
    // SEE_MASK_NOASYNC the call has finished with them when it returns.
    let done = unsafe { ShellExecuteExW(&mut info) };
    if done != 0 {
        return Ok(ShellStarted::Started);
    }
    let code = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
    if code == ERROR_CANCELLED {
        Ok(ShellStarted::Cancelled)
    } else {
        Err(u32::try_from(code).unwrap_or(0))
    }
}
