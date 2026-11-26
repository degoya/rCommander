//! `CopyFileExW` with its progress routine, the one Win32 call here without a safe wrapper:
//! winsafe's `CopyFile` has neither progress nor cancel. Within one SMB share the server copies
//! the file itself (server-side copy); the routine is called after each chunk, also of a copy the
//! server does.

use std::{ffi::c_void, io, os::windows::ffi::OsStrExt, path::Path, time::SystemTime};

use windows_sys::Win32::{
    Foundation::HANDLE,
    Storage::FileSystem::{
        COPY_FILE_FAIL_IF_EXISTS, COPYPROGRESSROUTINE_PROGRESS, CopyFileExW,
        LPPROGRESS_ROUTINE_CALLBACK_REASON, PROGRESS_CANCEL, PROGRESS_CONTINUE,
    },
};
use winsafe::{self as w, co};

use super::{cancelled, remove_unfinished};
use crate::CopyWatch;

/// `ERROR_REQUEST_ABORTED`: the progress routine cancelled the copy.
const ERROR_REQUEST_ABORTED: i32 = 1235;

/// See [`super::copy`].
pub(super) fn copy(from: &Path, to: &Path, watch: &CopyWatch) -> io::Result<u64> {
    // winsafe sets the attributes by `&str`: a name that is no UTF-8 goes the stream's way.
    let Some(name) = to.to_str() else {
        return Err(io::ErrorKind::Unsupported.into());
    };
    copy_file_ex(&wide(from), &wide(to), watch).inspect_err(|error| {
        // A target that was there is not this copy's; `CopyFileExW` removes its own on a
        // failure, this is for the one it could not.
        if error.kind() != io::ErrorKind::AlreadyExists {
            remove_unfinished(to);
        }
    })?;
    if let Err(error) = as_new_file(name, to) {
        remove_unfinished(to);
        return Err(error);
    }
    Ok(std::fs::metadata(to)?.len())
}

/// Gives the copy what any new file has instead of what `CopyFileExW` carried over: the archive
/// flag alone (no read-only, hidden or system flag) and the modification time of now.
fn as_new_file(name: &str, to: &Path) -> io::Result<()> {
    w::SetFileAttributes(name, co::FILE_ATTRIBUTE::ARCHIVE)
        .map_err(|error| io::Error::from_raw_os_error(u32::from(error) as i32))?;
    std::fs::OpenOptions::new()
        .write(true)
        .open(to)?
        .set_modified(SystemTime::now())
}

/// `path` as the NUL-terminated UTF-16 a `W` call takes.
fn wide(path: &Path) -> Vec<u16> {
    path.as_os_str().encode_wide().chain(Some(0)).collect()
}

#[allow(
    unsafe_code,
    reason = "CopyFileExW has no safe wrapper in std, winsafe or windows-sys with a progress \
              routine: it reads two NUL-terminated names that live for the call and hands \
              `lpdata` - the watch, borrowed for the call - only to `progress`, on this thread"
)]
fn copy_file_ex(from: &[u16], to: &[u16], watch: &CopyWatch) -> io::Result<()> {
    let data: *const c_void = std::ptr::from_ref(watch).cast();
    // SAFETY: `from` and `to` are NUL-terminated UTF-16 buffers alive for the whole call;
    // `progress` has the routine's signature; `data` points at `watch`, which outlives the call,
    // and the routine is only called during it; no cancel flag is passed (the routine cancels).
    let done = unsafe {
        CopyFileExW(
            from.as_ptr(),
            to.as_ptr(),
            Some(progress),
            data,
            std::ptr::null_mut(),
            COPY_FILE_FAIL_IF_EXISTS,
        )
    };
    if done != 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if watch.is_cancelled() || error.raw_os_error() == Some(ERROR_REQUEST_ABORTED) {
        return Err(cancelled());
    }
    Err(error)
}

/// `CopyFileExW`'s progress routine, at the start of each stream and after each chunk: the
/// bytes of the file's data (stream 1) to the watch; waits while the copy is paused, cancels
/// it once the watch says so.
#[allow(
    unsafe_code,
    reason = "reads `data` back as the `&CopyWatch` copy_file_ex passed to CopyFileExW, which \
              calls this only during that call"
)]
extern "system" fn progress(
    _total_size: i64,
    _total_done: i64,
    _stream_size: i64,
    stream_done: i64,
    stream: u32,
    _reason: LPPROGRESS_ROUTINE_CALLBACK_REASON,
    _source: HANDLE,
    _target: HANDLE,
    data: *const c_void,
) -> COPYPROGRESSROUTINE_PROGRESS {
    // SAFETY: `data` is the `&CopyWatch` `copy_file_ex` passed as `lpdata`; it lives for the
    // whole `CopyFileExW` call, the only time this routine runs, and is only read.
    let watch = unsafe { &*data.cast::<CopyWatch>() };
    if stream == 1 {
        watch.report(u64::try_from(stream_done).unwrap_or(0));
    }
    if watch.go_on() {
        PROGRESS_CONTINUE
    } else {
        PROGRESS_CANCEL
    }
}
