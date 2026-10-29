//! Files on the Windows clipboard (RC-080-01): `CF_HDROP` and "Preferred DropEffect", the two
//! formats the Explorer writes for Ctrl+C and Ctrl+X and reads for Ctrl+V; and a screenshot's
//! `CF_DIB` for the terminal's image paste (RC-040-08, `clipboard_image.rs`). The bytes are made and
//! read by `dropfiles.rs`; this module only moves them between the clipboard and global memory.
//!
//! The clipboard is opened for the app's window (`EmptyClipboard` makes it the owner, without
//! which `SetClipboardData` fails), inside `iced::window::run` on the window's thread, and
//! closed again before the call returns. What another program left there is read bounded: at
//! most [`MAX_BYTES`] of one format.

#![allow(
    unsafe_code,
    reason = "the clipboard exists only as Win32 calls on global memory, unsafe fns of \
              windows-sys; every block says why it holds"
)]

use std::{
    ffi::OsString,
    mem::ManuallyDrop,
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::PathBuf,
    ptr, slice,
};

use iced::Task;
use windows_sys::Win32::{
    Foundation::{GlobalFree, HGLOBAL, HWND},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, GetClipboardData, OpenClipboard,
            RegisterClipboardFormatW, SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock},
        Ole::{CF_DIB, CF_HDROP},
    },
};

use super::context_menu::{succeeded, wide};
use crate::{
    native_window,
    platform::{clipboard_files::ClipboardFiles, dropfiles},
};

/// The most bytes read from one format another program put on the clipboard.
const MAX_BYTES: usize = 64 * 1024 * 1024;
/// The registered format of the copy-or-move hint (`CFSTR_PREFERREDDROPEFFECT`).
const PREFERRED_EFFECT: &str = "Preferred DropEffect";

/// Puts `files` on the clipboard; whether it took them.
pub fn write(files: ClipboardFiles) -> Task<bool> {
    on_window(move |hwnd| put(hwnd, &files)).map(|done| done.is_some())
}

/// The files on the clipboard, if it holds any.
pub fn read() -> Task<Option<ClipboardFiles>> {
    on_window(get)
}

/// The device-independent bitmap on the clipboard, if it holds one (RC-040-08); a screenshot
/// is one. Bounded like every format read here.
pub fn read_dib() -> Task<Option<Vec<u8>>> {
    on_window(|hwnd| {
        let _open = Open::new(hwnd)?;
        take(u32::from(CF_DIB))
    })
}

/// Runs `work` with the app's window on its thread; `None` without a window.
fn on_window<T: Send + 'static>(
    work: impl Fn(HWND) -> Option<T> + Clone + Send + 'static,
) -> Task<Option<T>> {
    native_window::on_window(
        move |window| work(native_window::raw_hwnd(native_window::hwnd_of(window)?)),
        || None,
    )
}

fn put(hwnd: HWND, files: &ClipboardFiles) -> Option<()> {
    let paths: Vec<Vec<u16>> = files
        .paths
        .iter()
        .map(|path| path.as_os_str().encode_wide().collect())
        .collect();
    let list = dropfiles::encode(&paths);
    let effect = dropfiles::effect_bytes(files.cut);
    let preferred = preferred_format()?;
    let _open = Open::new(hwnd)?;
    // SAFETY: the clipboard is open for this thread; emptying it makes `hwnd` its owner.
    succeeded(unsafe { EmptyClipboard() })?;
    place(u32::from(CF_HDROP), &list)?;
    place(preferred, &effect)
}

fn get(hwnd: HWND) -> Option<ClipboardFiles> {
    let preferred = preferred_format();
    let _open = Open::new(hwnd)?;
    let list = take(u32::from(CF_HDROP))?;
    let paths = dropfiles::decode(&list)?
        .into_iter()
        .map(|units| PathBuf::from(OsString::from_wide(&units)))
        .collect();
    let cut = preferred
        .and_then(take)
        .is_some_and(|effect| dropfiles::is_move(&effect));
    Some(ClipboardFiles { paths, cut })
}

/// Puts `bytes` on the open clipboard as `format`.
fn place(format: u32, bytes: &[u8]) -> Option<()> {
    let memory = Global::copy_of(bytes)?;
    // SAFETY: the clipboard is open and ours, `memory` a moveable block that is not locked. On
    // success the system owns the block; on failure it stays ours and `memory` frees it.
    let placed = unsafe { SetClipboardData(format, memory.0) };
    if placed.is_null() {
        return None;
    }
    let _owned_by_the_clipboard = ManuallyDrop::new(memory);
    Some(())
}

/// A copy of what the open clipboard holds as `format`.
fn take(format: u32) -> Option<Vec<u8>> {
    // SAFETY: the clipboard is open; the handle stays the clipboard's, it is only read.
    let handle = unsafe { GetClipboardData(format) };
    if handle.is_null() {
        return None;
    }
    // SAFETY: a handle of a global-memory format; its size in bytes, 0 when it is none.
    let size = unsafe { GlobalSize(handle) };
    if size == 0 || size > MAX_BYTES {
        return None;
    }
    // SAFETY: locks the block for reading while the clipboard is open.
    let at = unsafe { GlobalLock(handle) };
    if at.is_null() {
        return None;
    }
    // SAFETY: the locked block holds `size` readable bytes until the unlock below.
    let bytes = unsafe { slice::from_raw_parts(at.cast::<u8>(), size) }.to_vec();
    // SAFETY: pairs the lock above; the result says only whether other locks remain.
    let _ = unsafe { GlobalUnlock(handle) };
    Some(bytes)
}

/// The id of the "Preferred DropEffect" format, registered by the shell long since.
fn preferred_format() -> Option<u32> {
    let name = wide(PREFERRED_EFFECT);
    // SAFETY: the name is NUL-terminated and outlives the call.
    let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
    (format != 0).then_some(format)
}

/// The clipboard, open for this thread until dropped.
struct Open;

impl Open {
    fn new(hwnd: HWND) -> Option<Self> {
        // SAFETY: `hwnd` is the live window of this thread; while another program has the
        // clipboard open this fails, and nothing is done.
        succeeded(unsafe { OpenClipboard(hwnd) })?;
        Some(Self)
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: closes the clipboard this thread opened in `new`.
        let _ = unsafe { CloseClipboard() };
    }
}

/// A moveable block of global memory of ours, freed unless the clipboard took it.
struct Global(HGLOBAL);

impl Global {
    /// A block holding a copy of `bytes` (never empty: a list has its header).
    fn copy_of(bytes: &[u8]) -> Option<Self> {
        // SAFETY: no pointers; the block is ours until freed or handed to the clipboard.
        let memory = Self(unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) });
        if memory.0.is_null() {
            return None;
        }
        // SAFETY: a moveable block of ours, locked for the copy.
        let at = unsafe { GlobalLock(memory.0) };
        if at.is_null() {
            return None;
        }
        // SAFETY: `at` points at the block's `bytes.len()` writable bytes, which `bytes`, our
        // own slice, does not overlap.
        unsafe { ptr::copy_nonoverlapping(bytes.as_ptr(), at.cast::<u8>(), bytes.len()) };
        // SAFETY: pairs the lock above; the result says only whether other locks remain.
        let _ = unsafe { GlobalUnlock(memory.0) };
        Some(memory)
    }
}

impl Drop for Global {
    fn drop(&mut self) {
        // SAFETY: the block is ours, unlocked and freed once, here.
        let _ = unsafe { GlobalFree(self.0) };
    }
}
