//! Windows: network drives through the WNet functions of `mpr.dll` (RC-010-16).
//!
//! `unsafe` is allowed in this module, and only here: `windows-sys` offers `WNetAddConnection2W`,
//! `WNetCancelConnection2W`, `WNetGetConnectionW` and the `WNetOpenEnumW` family as bare
//! `extern` functions, `WNetRestoreSingleConnectionW` is declared here, and `winsafe` (which
//! keeps the rest of `drives/windows.rs` safe) has no WNet functions. Every string handed over is
//! a NUL-terminated UTF-16 buffer owned by the calling function and alive for the whole call;
//! every call has its `SAFETY:` note. The functions that connect block - for as long as the
//! person takes in Windows' credential dialog - so they run on the blocking pool only.
//!
//! A drive mapped with *Reconnect at sign-in* is not connected at sign-in: Windows remembers it
//! and connects it the first time Explorer or Total Commander opens it
//! (`WNetRestoreSingleConnectionW`). Until then `GetLogicalDrives` does not have its letter, or
//! the drive does not answer; [`remembered`] lists it and [`restore`] connects it.
#![allow(
    unsafe_code,
    reason = "the WNet functions of mpr.dll are bare extern fns, winsafe has none; every block \
              has its SAFETY note"
)]

use std::{
    ffi::{OsStr, c_void},
    mem::size_of,
    os::windows::ffi::OsStrExt,
    ptr::{null, null_mut},
};

use windows_sys::Win32::{
    Foundation::{ERROR_CONNECTION_UNAVAIL, ERROR_MORE_DATA, ERROR_NO_MORE_ITEMS, NO_ERROR},
    NetworkManagement::WNet::{
        CONNECT_INTERACTIVE, CONNECT_PROMPT, CONNECT_UPDATE_PROFILE, NETRESOURCEW,
        RESOURCE_REMEMBERED, RESOURCETYPE_DISK, WNetAddConnection2W, WNetCancelConnection2W,
        WNetCloseEnum, WNetEnumResourceW, WNetGetConnectionW, WNetOpenEnumW,
    },
};

#[link(name = "mpr", kind = "raw-dylib")]
unsafe extern "system" {
    /// Connects the remembered drive `device` (`Z:`) as Explorer does on its first use; with
    /// `use_ui` Windows asks for credentials it does not have. `windows-sys` does not have it.
    fn WNetRestoreSingleConnectionW(parent: *mut c_void, device: *const u16, use_ui: i32) -> u32;
}

/// The bytes of the buffer the remembered drives are read into at first.
const ENUM_BUFFER: usize = 16 * 1024;

/// The longest remote name read back, in UTF-16 units; a longer one is not shown.
const REMOTE_CAPACITY: u32 = 1024;

/// Maps `remote` to `letter`. Windows asks for the credentials itself (`CONNECT_INTERACTIVE |
/// CONNECT_PROMPT`) and remembers them; `reconnect` keeps the drive across sign-ins
/// (`CONNECT_UPDATE_PROFILE`). The error is Windows' code.
pub(super) fn map(letter: char, remote: &str, reconnect: bool) -> Result<(), u32> {
    let mut local = wide(&format!("{letter}:"));
    let mut remote = wide(remote);
    let resource = NETRESOURCEW {
        dwType: RESOURCETYPE_DISK,
        lpLocalName: local.as_mut_ptr(),
        lpRemoteName: remote.as_mut_ptr(),
        ..NETRESOURCEW::default()
    };
    let mut flags = CONNECT_INTERACTIVE | CONNECT_PROMPT;
    if reconnect {
        flags |= CONNECT_UPDATE_PROFILE;
    }
    // SAFETY: `resource` points at `local` and `remote`, NUL-terminated buffers that outlive the
    // call; the password and user name are null, so Windows asks the person for them.
    let answer = unsafe { WNetAddConnection2W(&resource, null(), null(), flags) };
    if answer == NO_ERROR {
        Ok(())
    } else {
        Err(answer)
    }
}

/// Disconnects the drive `letter` and forgets it for the next sign-in. Without `force` Windows
/// refuses while files on it are open (`ERROR_OPEN_FILES`, `ERROR_DEVICE_IN_USE`).
pub(super) fn unmap(letter: char, force: bool) -> Result<(), u32> {
    let local = wide(&format!("{letter}:"));
    // SAFETY: `local` is a NUL-terminated buffer that outlives the call.
    let answer =
        unsafe { WNetCancelConnection2W(local.as_ptr(), CONNECT_UPDATE_PROFILE, i32::from(force)) };
    if answer == NO_ERROR {
        Ok(())
    } else {
        Err(answer)
    }
}

/// The folder the drive `letter` is mapped to (`\\server\share`), also while it is not
/// connected (a remembered drive); `None` for a drive that is not a network drive.
pub(super) fn remote_of(letter: char) -> Option<String> {
    let local = wide(&format!("{letter}:"));
    let mut buffer = vec![0u16; REMOTE_CAPACITY as usize];
    let mut length = REMOTE_CAPACITY;
    // SAFETY: `local` is NUL-terminated; `buffer` holds `length` UTF-16 units, the size Windows
    // is told, and both outlive the call.
    let answer = unsafe { WNetGetConnectionW(local.as_ptr(), buffer.as_mut_ptr(), &mut length) };
    if answer != NO_ERROR && answer != ERROR_CONNECTION_UNAVAIL {
        if answer == ERROR_MORE_DATA {
            tracing::debug!(%letter, "the network drive's folder is too long to show");
        }
        return None;
    }
    let end = buffer.iter().position(|&unit| unit == 0)?;
    Some(String::from_utf16_lossy(&buffer[..end])).filter(|remote| !remote.is_empty())
}

/// Connects the remembered network drive `letter`, Windows asking for credentials it does not
/// have. The error is Windows' code.
pub(super) fn restore(letter: char) -> Result<(), u32> {
    let local = wide(&format!("{letter}:"));
    // SAFETY: `local` is a NUL-terminated buffer that outlives the call; no parent window.
    let answer = unsafe { WNetRestoreSingleConnectionW(null_mut(), local.as_ptr(), 1) };
    if answer == NO_ERROR {
        Ok(())
    } else {
        Err(answer)
    }
}

/// The network drives Windows remembers for this user, connected or not: each letter with its
/// folder. Reads the profile, never the network.
pub(super) fn remembered() -> Vec<(char, String)> {
    let mut handle = null_mut();
    // SAFETY: a null resource asks for the top of the remembered connections; `handle` is
    // written by the call and closed below.
    let answer = unsafe {
        WNetOpenEnumW(
            RESOURCE_REMEMBERED,
            RESOURCETYPE_DISK,
            0,
            null(),
            &mut handle,
        )
    };
    if answer != NO_ERROR {
        tracing::debug!(
            code = answer,
            "the remembered network drives cannot be read"
        );
        return Vec::new();
    }
    let mut found = Vec::new();
    // `u64`s keep the buffer aligned for the `NETRESOURCEW`s Windows writes at its start.
    let mut buffer = vec![0u64; ENUM_BUFFER / size_of::<u64>()];
    loop {
        let mut count = u32::MAX;
        let mut size = u32::try_from(buffer.len() * size_of::<u64>()).unwrap_or(u32::MAX);
        // SAFETY: `buffer` holds `size` bytes, the size Windows is told, and outlives the call.
        let answer =
            unsafe { WNetEnumResourceW(handle, &mut count, buffer.as_mut_ptr().cast(), &mut size) };
        if answer == ERROR_MORE_DATA {
            // Not even one entry fits: `size` is the bytes it needs.
            buffer.resize((size as usize).div_ceil(size_of::<u64>()), 0);
            continue;
        }
        if answer != NO_ERROR {
            if answer != ERROR_NO_MORE_ITEMS {
                tracing::debug!(code = answer, "the remembered network drives stop early");
            }
            break;
        }
        let resources: *const NETRESOURCEW = buffer.as_ptr().cast();
        for at in 0..count as usize {
            // SAFETY: Windows wrote `count` `NETRESOURCEW`s at the buffer's start; their names
            // point at NUL-terminated strings in the same buffer, or are null.
            let (local, remote) = unsafe {
                let resource = &*resources.add(at);
                (
                    text_at(resource.lpLocalName),
                    text_at(resource.lpRemoteName),
                )
            };
            if let (Some(letter), Some(remote)) = (local.as_deref().and_then(letter_of), remote) {
                found.push((letter, remote));
            }
        }
    }
    // SAFETY: `handle` was opened above and is closed once.
    unsafe { WNetCloseEnum(handle) };
    found
}

/// The letter of a local name `Z:`.
fn letter_of(local: &str) -> Option<char> {
    rc_vfs::drive_letter(local)
        .filter(|_| local.len() == 2)
        .map(|letter| letter.to_ascii_uppercase())
}

/// The NUL-terminated UTF-16 string at `at`; `None` for a null pointer or an empty string.
///
/// # Safety
/// `at` is null or points at a NUL-terminated UTF-16 string.
unsafe fn text_at(at: *const u16) -> Option<String> {
    if at.is_null() {
        return None;
    }
    let mut length = 0;
    // SAFETY: the string ends with a NUL, so every unit up to it is readable.
    while unsafe { *at.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the `length` units before the NUL were just read.
    let units = unsafe { std::slice::from_raw_parts(at, length) };
    Some(String::from_utf16_lossy(units)).filter(|text| !text.is_empty())
}

/// `text` as a NUL-terminated UTF-16 string.
fn wide(text: &str) -> Vec<u16> {
    OsStr::new(text).encode_wide().chain([0]).collect()
}
