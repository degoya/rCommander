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

use std::{
    ffi::OsStr,
    os::windows::ffi::OsStrExt,
    ptr::{null, null_mut},
};

use windows_sys::Win32::{
    Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC,
    },
    Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_NORMAL, FILE_FLAGS_AND_ATTRIBUTES,
    },
    System::Com::{
        COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize,
    },
    UI::{
        Shell::{
            SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
        },
        WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO},
    },
};

use super::{
    key::IconKey,
    loader::IconProvider,
    pixels::{self, Rgba},
};

/// The largest icon side taken; a small icon is at most 64 px even at 400 %.
const MAX_SIDE: u32 = 256;

/// The shell's icons; made and used on the loader thread, which it initialises COM on.
pub struct ShellIcons {
    com: bool,
}

impl ShellIcons {
    /// Initialises COM on the calling thread, the loader's.
    pub fn start() -> Self {
        // SAFETY: called on the loader thread that owns this value for its whole life; a
        // success is paired with `CoUninitialize` in `drop` on the same thread.
        let result = unsafe {
            CoInitializeEx(
                null(),
                (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32,
            )
        };
        Self { com: result >= 0 }
    }
}

impl Drop for ShellIcons {
    fn drop(&mut self) {
        if self.com {
            // SAFETY: balances the successful `CoInitializeEx` of `start` on this thread.
            unsafe { CoUninitialize() };
        }
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

/// An icon handle this module owns.
struct Icon(HICON);

impl Drop for Icon {
    fn drop(&mut self) {
        // SAFETY: the handle came from SHGetFileInfoW with SHGFI_ICON and is destroyed once.
        unsafe { DestroyIcon(self.0) };
    }
}

/// A bitmap handle this module owns; null for none.
struct Bitmap(HBITMAP);

impl Drop for Bitmap {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: GetIconInfo created the bitmap for the caller, who deletes it once; it is
            // selected into no DC.
            unsafe { DeleteObject(self.0) };
        }
    }
}

/// A memory device context this module owns.
struct MemoryDc(HDC);

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: made by CreateCompatibleDC below, deleted once.
        unsafe { DeleteDC(self.0) };
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
        let color = Bitmap(info.hbmColor);
        let mask = Bitmap(info.hbmMask);
        // A monochrome icon has no colour bitmap; the generic icon stands in.
        if color.0.is_null() {
            return None;
        }
        let mut bitmap = BITMAP::default();
        // SAFETY: `color.0` is a valid bitmap; the buffer is a BITMAP of the size passed.
        let read = unsafe {
            GetObjectW(
                color.0,
                size_of::<BITMAP>() as i32,
                (&raw mut bitmap).cast(),
            )
        };
        if read == 0 {
            return None;
        }
        let width = u32::try_from(bitmap.bmWidth).ok()?;
        let height = u32::try_from(bitmap.bmHeight).ok()?;
        if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
            return None;
        }
        // SAFETY: a null DC asks for a memory DC compatible with the screen.
        let dc = MemoryDc(unsafe { CreateCompatibleDC(null_mut()) });
        if dc.0.is_null() {
            return None;
        }
        let colors = dib_pixels(&dc, &color, width, height)?;
        let mask = if mask.0.is_null() {
            None
        } else {
            dib_pixels(&dc, &mask, width, height)
        };
        pixels::from_bgra(width, height, &colors, mask.as_deref())
    }
}

/// The pixels of `bitmap` as 32-bit BGRA rows from the top.
fn dib_pixels(dc: &MemoryDc, bitmap: &Bitmap, width: u32, height: u32) -> Option<Vec<u8>> {
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            // Negative: top-down rows.
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    };
    let mut pixels = vec![0u8; width as usize * height as usize * 4];
    // SAFETY: `pixels` holds `height` rows of `width` 32-bit pixels, exactly what a 32 bpp
    // BI_RGB request for `height` lines writes (32 bpp rows need no padding); `info` describes
    // that layout; the bitmap is selected into no DC; `dc` is a valid memory DC.
    let lines = unsafe {
        GetDIBits(
            dc.0,
            bitmap.0,
            0,
            height,
            pixels.as_mut_ptr().cast(),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    (u32::try_from(lines).ok() == Some(height)).then_some(pixels)
}
