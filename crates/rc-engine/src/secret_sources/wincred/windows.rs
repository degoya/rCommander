//! Windows: the generic credentials of the signed-in person through `advapi32.dll` (RC-100-37).
//!
//! `unsafe` is allowed in this module, and only here: `windows-sys` offers `CredEnumerateW`,
//! `CredReadW` and `CredFree` as bare `extern` functions and `winsafe` has none of them. Every
//! buffer Windows hands out is copied while it is valid and released with `CredFree` exactly
//! once; the blob is overwritten before it is released, so the secret stays only in the
//! `Zeroizing` copy. Every block has its `SAFETY:` note.
#![allow(
    unsafe_code,
    reason = "the credential functions of advapi32.dll are bare extern fns, winsafe has none; \
              every block has its SAFETY note"
)]

use std::{
    ffi::{OsStr, c_void},
    os::windows::ffi::OsStrExt,
    ptr::{null, null_mut},
};

use rc_plugin_types::{Failure, FailureKind};
use windows_sys::{
    Win32::{
        Foundation::{ERROR_NOT_FOUND, GetLastError},
        Security::Credentials::{
            CRED_TYPE_GENERIC, CREDENTIALW, CredEnumerateW, CredFree, CredReadW,
        },
    },
    core::PWSTR,
};
use zeroize::Zeroizing;

use super::Credential;

/// Every generic credential of the signed-in person, without blobs.
pub(super) fn credentials() -> Result<Vec<Credential>, Failure> {
    let mut count = 0u32;
    let mut list: *mut *mut CREDENTIALW = null_mut();
    // SAFETY: a null filter enumerates every credential; `count` and `list` are valid out
    // pointers. On success Windows allocates `list`, released below with `CredFree`.
    if unsafe { CredEnumerateW(null(), 0, &mut count, &mut list) } == 0 {
        // SAFETY: reads the calling thread's last error, set by the call above.
        let error = unsafe { GetLastError() };
        return if error == ERROR_NOT_FOUND {
            Ok(Vec::new())
        } else {
            Err(failed(error))
        };
    }
    let mut found = Vec::new();
    for index in 0..count as usize {
        // SAFETY: `list` holds `count` valid pointers to `CREDENTIALW`, alive until `CredFree`.
        let credential = unsafe { &**list.add(index) };
        if credential.Type != CRED_TYPE_GENERIC {
            continue;
        }
        // SAFETY: the strings of a listed credential are NUL-terminated or null, alive until
        // `CredFree`.
        let (target, user, comment) = unsafe {
            (
                text(credential.TargetName),
                text(credential.UserName),
                text(credential.Comment),
            )
        };
        if let Some(target) = target {
            found.push(Credential {
                target,
                user,
                comment,
            });
        }
    }
    // SAFETY: `list` came from `CredEnumerateW` and is released once; nothing points into it.
    unsafe { CredFree(list as *const c_void) };
    Ok(found)
}

/// The blob of the generic credential `target`.
pub(super) fn read(target: &str) -> Result<Zeroizing<Vec<u8>>, Failure> {
    let name: Vec<u16> = OsStr::new(target).encode_wide().chain(Some(0)).collect();
    let mut credential: *mut CREDENTIALW = null_mut();
    // SAFETY: `name` is a NUL-terminated UTF-16 buffer alive for the call; `credential` is a
    // valid out pointer. On success Windows allocates it, released below with `CredFree`.
    if unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) } == 0 {
        // SAFETY: reads the calling thread's last error, set by the call above.
        let error = unsafe { GetLastError() };
        return Err(if error == ERROR_NOT_FOUND {
            Failure::new(FailureKind::NotFound)
        } else {
            failed(error)
        });
    }
    // SAFETY: `credential` points to a valid `CREDENTIALW` until `CredFree`.
    let (blob, size) = unsafe {
        (
            (*credential).CredentialBlob,
            (*credential).CredentialBlobSize,
        )
    };
    let mut copy = Zeroizing::new(Vec::new());
    if !blob.is_null() && size > 0 {
        // SAFETY: `blob` holds `size` bytes, owned by the credential until `CredFree`; the
        // slice is dropped before the bytes are overwritten.
        copy.extend_from_slice(unsafe { std::slice::from_raw_parts(blob, size as usize) });
        // SAFETY: the same `size` bytes, writable until `CredFree`: the secret leaves no copy.
        unsafe { std::ptr::write_bytes(blob, 0, size as usize) };
    }
    // SAFETY: `credential` came from `CredReadW` and is released once; nothing points into it.
    unsafe { CredFree(credential as *const c_void) };
    Ok(copy)
}

/// The text at `pointer`, `None` for null, empty or not UTF-16 (a target read back lossily
/// would name another credential).
///
/// # Safety
/// `pointer` is null or a NUL-terminated UTF-16 string alive for the call.
unsafe fn text(pointer: PWSTR) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    let mut length = 0;
    // SAFETY: the string is NUL-terminated (the caller's promise), so every unit up to the
    // NUL is readable.
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: `length` units from `pointer` were just read.
    let units = unsafe { std::slice::from_raw_parts(pointer, length) };
    String::from_utf16(units)
        .ok()
        .filter(|text| !text.is_empty())
}

/// A call that failed with Windows' `error`.
fn failed(error: u32) -> Failure {
    Failure::coded(FailureKind::Io, "wincred_failed").with_param("error", error)
}
