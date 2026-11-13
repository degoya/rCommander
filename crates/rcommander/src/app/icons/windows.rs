//! Windows: the icons Explorer shows, from the shell.
//!
//! `unsafe` is allowed in this module, and only here: `windows-sys` offers the shell and GDI
//! calls as bare `extern` functions with no safe wrapper (the crate already in the tree; `winsafe`
//! has no shell icons in its features). Every handle is owned by a guard that frees it, every
//! call has its `SAFETY:` note.
//!
//! `SHGetFileInfoW` gives an icon by attributes (`SHGFI_USEFILEATTRIBUTES`: an extension or a
//! folder, no file touched) or of a file itself (a program's, a shortcut's, an icon file's, a
//! folder's `desktop.ini`). `SHGFI_SMALLICON` is the size of the system's small icons, which
//! Windows scales with the display: 16 px at 100 %, 24 px at 150 %, 32 px at 200 % - the pixels
//! the list's 16-point icon cell has. The `HICON` becomes RGBA through its colour and mask
//! bitmaps (`GetIconInfo`, `GetDIBits`), alpha kept.
#![allow(
    unsafe_code,
    reason = "the shell and GDI icon calls are bare extern fns of windows-sys; every handle has a \
              guard, every block its SAFETY note"
)]

use std::{ffi::OsStr, os::windows::ffi::OsStrExt, path::Path, ptr::null_mut};

use windows_sys::Win32::{
    Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
    },
    UI::{
        Shell::{
            ExtractIconExW, SHDefExtractIconW, SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON,
            SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
        },
        WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
    },
};

use super::{
    key::IconKey,
    loader::IconProvider,
    pixels::{self, Rgba},
};
use crate::platform::gdi::{Bitmap, Com, MemoryDc};

/// The largest icon side taken; a small icon is at most 64 px even at 400 %.
const MAX_SIDE: u32 = 256;

/// The shell's icons; made and used on the loader thread, which it initialises COM on.
pub struct ShellIcons {
    _com: Com,
}

impl ShellIcons {
    /// Initialises COM on the calling thread, the loader's.
    pub fn start() -> Self {
        // The loader thread owns this value for its whole life: the guard is made and dropped
        // on it.
        Self { _com: Com::init() }
    }
}

impl IconProvider for ShellIcons {
    fn load(&self, key: &IconKey) -> Option<Rgba> {
        match key {
            IconKey::Folder => shell_icon(OsStr::new("folder"), Some(FILE_ATTRIBUTE_DIRECTORY)),
            IconKey::Ext(ext) => by_extension(ext),
            // A file the shell cannot read (gone, no access) shows its type's icon.
            IconKey::File(path) => shell_icon(path.as_os_str(), None).or_else(|| {
                let ext = path.extension().and_then(OsStr::to_str);
                by_extension(&ext.unwrap_or_default().to_ascii_lowercase())
            }),
        }
    }
}

/// The icon of the file type `ext`, without a file.
fn by_extension(ext: &str) -> Option<Rgba> {
    let name = if ext.is_empty() {
        "file".to_owned()
    } else {
        format!("file.{ext}")
    };
    shell_icon(OsStr::new(&name), Some(FILE_ATTRIBUTE_NORMAL))
}

/// The small icon of `name`: of the file itself, or by `attributes` without touching it.
fn shell_icon(name: &OsStr, attributes: Option<FILE_FLAGS_AND_ATTRIBUTES>) -> Option<Rgba> {
    let wide: Vec<u16> = name.encode_wide().chain(Some(0)).collect();
    let mut info = SHFILEINFOW::default();
    let mut flags = SHGFI_ICON | SHGFI_SMALLICON;
    if attributes.is_some() {
        flags |= SHGFI_USEFILEATTRIBUTES;
    }
    // SAFETY: `wide` is NUL-terminated and outlives the call; `info` is a writable SHFILEINFOW
    // of the size passed. On success the call hands over `info.hIcon`, which `Icon` frees.
    let found = unsafe {
        SHGetFileInfoW(
            wide.as_ptr(),
            attributes.unwrap_or(0),
            &mut info,
            size_of::<SHFILEINFOW>() as u32,
            flags,
        )
    };
    if found == 0 || info.hIcon.is_null() {
        return None;
    }
    let icon = Icon(info.hIcon);
    icon.rgba()
}

/// The icon at `index` of `path` - an `.exe`, a `.dll` or an `.ico` -, a button's symbol
/// (RC-100-87): the large one (32 px at 100 %) with `large`, else the small one, the other where
/// the file has only one; a negative index is a resource id, as Total Commander reads it. `None`
/// when the file holds no such icon.
pub fn extracted(path: &Path, index: i32, large: bool) -> Option<Rgba> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut big: HICON = null_mut();
    let mut small: HICON = null_mut();
    // SAFETY: `wide` is NUL-terminated and outlives the call; both out-pointers are writable
    // HICONs and one icon of each size is asked for. The handles it returns belong to the
    // caller, which the guards below destroy.
    let found = unsafe { ExtractIconExW(wide.as_ptr(), index, &mut big, &mut small, 1) };
    // Only a handle the call gave is guarded: a null one is never destroyed.
    let big = if big.is_null() { None } else { Some(Icon(big)) };
    let small = if small.is_null() {
        None
    } else {
        Some(Icon(small))
    };
    if found == 0 || found == u32::MAX {
        return None;
    }
    let chosen = if large {
        big.as_ref().or(small.as_ref())
    } else {
        small.as_ref().or(big.as_ref())
    };
    chosen?.rgba()
}

/// The icon at `index` of `path` - an `.exe`, a `.dll` or an `.ico` - at `side` pixels, a
/// button's symbol at the bars' size (RC-110-15): the shell scales the file's next larger image
/// down to it, so a symbol stays sharp at 40 or 48 pixels as at 16, 24 or 32, as Total Commander
/// loads an unusual size. A negative index is a resource id. `None` when the file holds no such
/// icon.
pub fn extracted_at(path: &Path, index: i32, side: u32) -> Option<Rgba> {
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // The low word is the large icon's side; the high word, the small one's, stays 0: not asked.
    let side = side.clamp(1, MAX_SIDE);
    let mut icon: HICON = null_mut();
    // SAFETY: `wide` is NUL-terminated and outlives the call; `icon` is a writable HICON for the
    // large icon, and the small one's pointer is null, which the call allows. The handle it
    // returns belongs to the caller, which the guard below destroys.
    let result = unsafe { SHDefExtractIconW(wide.as_ptr(), index, 0, &mut icon, null_mut(), side) };
    // Only a handle the call gave is guarded: a null one is never destroyed.
    let icon = if icon.is_null() {
        None
    } else {
        Some(Icon(icon))
    };
    // S_OK; S_FALSE says the file has no such icon.
    if result != 0 {
        return None;
    }
    icon?.rgba()
}

/// The small icon of the file at `path` itself, as Explorer shows it: a program's own.
pub fn file_icon(path: &Path) -> Option<Rgba> {
    shell_icon(path.as_os_str(), None)
}

/// An icon handle this module owns.
struct Icon(HICON);

impl Drop for Icon {
    fn drop(&mut self) {
        // SAFETY: the handle came from SHGetFileInfoW with SHGFI_ICON, from ExtractIconExW or
        // from SHDefExtractIconW, and is destroyed once.
        unsafe { DestroyIcon(self.0) };
    }
}

impl Icon {
    /// The icon's pixels: its colour bitmap with alpha, or with the mask's transparency.
    fn rgba(&self) -> Option<Rgba> {
        let mut info = ICONINFO::default();
        // SAFETY: `self.0` is a valid icon; `info` is writable. On success the two bitmaps in
        // it belong to the caller, which the guards below delete.
        if unsafe { GetIconInfo(self.0, &mut info) } == 0 {
            return None;
        }
        let color = Bitmap::owned(info.hbmColor);
        let mask = Bitmap::owned(info.hbmMask);
        // A monochrome icon has no colour bitmap; the generic icon stands in.
        let (width, height) = color.dimensions()?;
        let width = u32::try_from(width).ok()?;
        let height = u32::try_from(height).ok()?;
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return None;
        }
        let dc = MemoryDc::screen()?;
        let colors = color.pixels(&dc, width, height)?;
        let mask = if mask.is_null() {
            None
        } else {
            mask.pixels(&dc, width, height)
        };
        pixels::from_bgra(width, height, &colors, mask.as_deref())
    }
}
