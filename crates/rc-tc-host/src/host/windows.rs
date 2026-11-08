//! The loaded DLL (Windows): `LoadLibraryExW` with the search limited to the DLL's own folder and
//! the system's, the kinds read from its exports, and the WCX and WLX functions bound by name.
//!
//! `unsafe` is allowed in this module and its children, and nowhere else in the crate: a TC
//! plugin is a C ABI behind `GetProcAddress`, which neither std nor `winsafe` can call safely.
//! Every block has its `SAFETY:` note. What the DLL does inside a call is beyond any note - that
//! is why it runs in this process and not in rCommander's.
#![allow(
    unsafe_code,
    reason = "a TC plugin is a C ABI reached through LoadLibraryExW and GetProcAddress, which \
              std and winsafe cannot call; every block has its SAFETY note"
)]

/// Binds the export `$name` of `$library` as the function type `$kind`; `None` when it is
/// missing.
macro_rules! bind {
    ($library:expr, $name:literal, $kind:ty) => {
        $library.symbol($name).map(|function| {
            // SAFETY: the export has the signature the TC plugin headers declare for `$name`,
            // which `$kind` repeats; the pointer is called only while the library is loaded.
            unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $kind>(function) }
        })
    };
}

mod wcx;
mod wlx;

use std::ffi::{CString, OsStr};
use std::fs::File;
use std::io::Read;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::ptr::null_mut;

use rc_tc_host::dll::{HEADER_BYTES, check_machine, kinds};
use rc_tc_host::rpc::{Entry, Kind};
use rc_tc_host::{Bitmap, PluginError, TcPlugin};
use windows_sys::Win32::Foundation::{FARPROC, FreeLibrary, HMODULE};
use windows_sys::Win32::System::LibraryLoader::{
    GetProcAddress, LOAD_LIBRARY_SEARCH_DEFAULT_DIRS, LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR,
    LoadLibraryExW,
};

/// The plugin interface versions the host announces (`PluginInterfaceVersionHi`/`Low`).
const WCX_VERSION: (u32, u32) = (2, 21);
const WLX_VERSION: (u32, u32) = (2, 12);

/// A loaded DLL, freed on drop.
pub struct Library(HMODULE);

impl Library {
    fn load(path: &Path) -> Result<Self, String> {
        let wide = wide(path.as_os_str());
        // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call; the flags limit
        // the search for the DLL's own imports to its folder and the system's.
        let module = unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                null_mut(),
                LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS,
            )
        };
        if module.is_null() {
            return Err(format!(
                "LoadLibraryExW failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(Self(module))
    }

    /// The export `name`, or `None`.
    fn symbol(&self, name: &str) -> FARPROC {
        let name = CString::new(name).ok()?;
        // SAFETY: the module is loaded while `self` lives; `name` is a NUL-terminated ASCII name
        // that outlives the call.
        unsafe { GetProcAddress(self.0, name.as_ptr().cast()) }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        // SAFETY: the handle came from `LoadLibraryExW` and is freed once; every function pointer
        // bound from it lives in a `DllPlugin` field dropped before this one.
        unsafe {
            FreeLibrary(self.0);
        }
    }
}

/// A NUL-terminated UTF-16 copy of `text`.
fn wide(text: &OsStr) -> Vec<u16> {
    text.encode_wide().chain(Some(0)).collect()
}

/// The text of a NUL-terminated UTF-16 buffer.
fn from_wide(buffer: &[u16]) -> String {
    let end = buffer
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// TC's `DefaultIniName`: an ANSI path of at most `MAX_PATH` bytes. A path outside ASCII is not
/// handed over (the plugin then keeps its default), since the ANSI code page is the machine's.
fn ini_name(path: &Path) -> Option<[u8; 260]> {
    let text = path
        .to_str()
        .filter(|text| text.is_ascii() && text.len() < 260)?;
    let mut name = [0_u8; 260];
    name[..text.len()].copy_from_slice(text.as_bytes());
    Some(name)
}

/// The DLL behind the host's loop. The bound functions come before the library so they drop
/// first.
pub struct DllPlugin {
    packer: Option<wcx::Packer>,
    lister: Option<wlx::Lister>,
    kinds: Vec<Kind>,
    _library: Library,
}

/// Loads `dll` after the same machine check rCommander made; `staging` holds the plugin's INI.
pub fn open(dll: &Path, staging: &Path) -> Result<DllPlugin, String> {
    let mut head = Vec::with_capacity(HEADER_BYTES);
    File::open(dll)
        .and_then(|file| file.take(HEADER_BYTES as u64).read_to_end(&mut head))
        .map_err(|error| error.to_string())?;
    check_machine(&head)?;
    let library = Library::load(dll)?;
    let kinds = kinds(|name| library.symbol(name).is_some());
    let packer = if kinds.contains(&Kind::Wcx) {
        wcx::Packer::bind(&library, ini_name(&staging.join("pkplugin.ini")))
    } else {
        None
    };
    let lister = if kinds.contains(&Kind::Wlx) {
        wlx::Lister::bind(&library, ini_name(&staging.join("lsplugin.ini")))
    } else {
        None
    };
    Ok(DllPlugin {
        packer,
        lister,
        kinds,
        _library: library,
    })
}

fn unsupported(kind: &str) -> PluginError {
    PluginError::Unsupported(format!("the plugin is no {kind} plugin"))
}

impl TcPlugin for DllPlugin {
    fn kinds(&self) -> Vec<Kind> {
        self.kinds.clone()
    }

    fn list(&mut self, archive: &Path) -> Result<Vec<Entry>, PluginError> {
        self.packer
            .as_ref()
            .ok_or_else(|| unsupported("packer"))?
            .list(archive)
    }

    fn extract(&mut self, archive: &Path, entry: &str, target: &Path) -> Result<(), PluginError> {
        self.packer
            .as_ref()
            .ok_or_else(|| unsupported("packer"))?
            .extract(archive, entry, target)
    }

    fn preview(&mut self, file: &Path, width: u32, height: u32) -> Result<Bitmap, PluginError> {
        self.lister
            .as_ref()
            .ok_or_else(|| unsupported("lister"))?
            .preview(file, width, height)
    }
}
