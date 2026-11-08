//! A lister DLL (WLX, `listplug.h`), for Quick View's preview only: `ListGetPreviewBitmapW` with
//! the file's first 8 KiB, as TC hands them, and the bitmap it returns read back as top-down BGRA
//! rows through `GetDIBits`. `ListLoadW` into a window of its own is not part of the spike.
//! `unsafe` is allowed by the parent module.

use std::ffi::c_void;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::ptr::null_mut;

use rc_tc_host::{Bitmap, PluginError};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW,
    HBITMAP, ReleaseDC,
};

use super::{Library, WLX_VERSION, wide};

/// The bytes of the file TC hands the plugin to recognise it.
const CONTENT: usize = 8 * 1024;

/// `ListDefaultParamStruct`.
#[repr(C)]
#[allow(
    dead_code,
    reason = "the C layout needs every field; the host reads none"
)]
struct ListDefaultParamStruct {
    size: i32,
    version_low: u32,
    version_high: u32,
    default_ini_name: [u8; 260],
}

type ListGetPreviewBitmapW = unsafe extern "system" fn(*mut u16, i32, i32, *mut u8, i32) -> HBITMAP;
type ListSetDefaultParams = unsafe extern "system" fn(*mut ListDefaultParamStruct);

/// The lister functions of a loaded DLL.
pub struct Lister {
    preview: ListGetPreviewBitmapW,
}

impl Lister {
    /// Binds the functions; `ListSetDefaultParams` is called once when the DLL has it.
    pub fn bind(library: &Library, ini: Option<[u8; 260]>) -> Option<Self> {
        let lister = Self {
            preview: bind!(library, "ListGetPreviewBitmapW", ListGetPreviewBitmapW)?,
        };
        if let (Some(defaults), Some(ini)) = (
            bind!(library, "ListSetDefaultParams", ListSetDefaultParams),
            ini,
        ) {
            let mut params = ListDefaultParamStruct {
                size: size_of::<ListDefaultParamStruct>() as i32,
                version_low: WLX_VERSION.1,
                version_high: WLX_VERSION.0,
                default_ini_name: ini,
            };
            // SAFETY: `params` is a `ListDefaultParamStruct` with its size set, alive for the call.
            unsafe { defaults(&mut params) };
        }
        Some(lister)
    }

    pub fn preview(&self, file: &Path, width: u32, height: u32) -> Result<Bitmap, PluginError> {
        let mut content = Vec::with_capacity(CONTENT);
        File::open(file)
            .and_then(|handle| handle.take(CONTENT as u64).read_to_end(&mut content))
            .map_err(|error| PluginError::Failed(format!("reading the file: {error}")))?;
        let mut name = wide(file.as_os_str());
        // SAFETY: `name` is a NUL-terminated path and `content` a buffer of the length passed,
        // both alive for the call; the sizes are at most `MAX_PREVIEW`, so they fit an `i32`.
        let bitmap = unsafe {
            (self.preview)(
                name.as_mut_ptr(),
                width as i32,
                height as i32,
                content.as_mut_ptr(),
                content.len() as i32,
            )
        };
        if bitmap.is_null() {
            return Err(PluginError::Failed(
                "the plugin has no preview of this file".to_owned(),
            ));
        }
        let pixels = read_bitmap(bitmap, width, height);
        // SAFETY: the bitmap is ours now (TC deletes it after use, and so does the host); it is
        // selected into no device context.
        unsafe { DeleteObject(bitmap) };
        pixels
    }
}

/// The pixels of `bitmap` as top-down BGRA rows; a bitmap larger than `max_width` x `max_height`
/// is refused before its pixels are allocated.
fn read_bitmap(bitmap: HBITMAP, max_width: u32, max_height: u32) -> Result<Bitmap, PluginError> {
    let mut info = BITMAP::default();
    // SAFETY: `info` is a `BITMAP` of the size passed, alive for the call.
    let read = unsafe {
        GetObjectW(
            bitmap,
            size_of::<BITMAP>() as i32,
            (&mut info as *mut BITMAP).cast::<c_void>(),
        )
    };
    if read == 0 || info.bmWidth <= 0 || info.bmHeight == 0 {
        return Err(PluginError::Failed("the preview is no bitmap".to_owned()));
    }
    let (width, height) = (info.bmWidth.unsigned_abs(), info.bmHeight.unsigned_abs());
    if width > max_width || height > max_height {
        return Err(PluginError::Failed(format!(
            "the plugin's preview of {width}x{height} does not fit {max_width}x{max_height}"
        )));
    }
    let mut header = BITMAPINFO::default();
    header.bmiHeader.biSize = size_of_val(&header.bmiHeader) as u32;
    header.bmiHeader.biWidth = info.bmWidth;
    // Negative: top row first.
    header.bmiHeader.biHeight = -(height as i32);
    header.bmiHeader.biPlanes = 1;
    header.bmiHeader.biBitCount = 32;
    header.bmiHeader.biCompression = BI_RGB;
    let mut bgra = vec![0_u8; width as usize * height as usize * 4];
    // SAFETY: the screen's device context is released below; `bgra` holds `height` rows of
    // `width` 32-bit pixels, exactly what `header` asks for; the bitmap is in no context.
    let rows = unsafe {
        let screen = GetDC(null_mut());
        let rows = GetDIBits(
            screen,
            bitmap,
            0,
            height,
            bgra.as_mut_ptr().cast::<c_void>(),
            &mut header,
            DIB_RGB_COLORS,
        );
        ReleaseDC(null_mut(), screen);
        rows
    };
    if rows != height as i32 {
        return Err(PluginError::Failed(
            "the preview's pixels could not be read".to_owned(),
        ));
    }
    Ok(Bitmap {
        width,
        height,
        bgra,
    })
}
