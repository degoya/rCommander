//! A packer DLL (WCX, `wcxhead.h`): an archive opened per request, its headers read in turn, each
//! entry skipped or extracted. Multi-volume archives are refused (`ChangeVolProcW` answers
//! "abort"), progress is not reported (`ProcessDataProcW` answers "go on"). `unsafe` is allowed
//! by the parent module.

use std::ffi::c_void;
use std::path::Path;
use std::ptr::null_mut;

use rc_tc_host::PluginError;
use rc_tc_host::rpc::Entry;
use rc_tc_host::wcx::{
    E_END_ARCHIVE, PK_EXTRACT, PK_OM_EXTRACT, PK_OM_LIST, PK_SKIP, dos_time, error_name, size,
};

use super::{Library, WCX_VERSION, from_wide, wide};

type Handle = *mut c_void;

/// `tOpenArchiveDataW`.
#[allow(
    dead_code,
    reason = "the C layout needs every field; the host reads only some"
)]
#[repr(C)]
struct OpenArchiveDataW {
    arc_name: *mut u16,
    open_mode: i32,
    open_result: i32,
    cmt_buf: *mut u16,
    cmt_buf_size: i32,
    cmt_size: i32,
    cmt_state: i32,
}

/// `tHeaderDataExW`.
#[allow(
    dead_code,
    reason = "the C layout needs every field; the host reads only some"
)]
#[repr(C)]
struct HeaderDataExW {
    arc_name: [u16; 1024],
    file_name: [u16; 1024],
    flags: i32,
    pack_size: u32,
    pack_size_high: u32,
    unp_size: u32,
    unp_size_high: u32,
    host_os: i32,
    file_crc: i32,
    file_time: i32,
    unp_ver: i32,
    method: i32,
    file_attr: i32,
    cmt_buf: *mut u8,
    cmt_buf_size: i32,
    cmt_size: i32,
    cmt_state: i32,
    reserved: [u8; 1024],
}

impl HeaderDataExW {
    fn empty() -> Box<Self> {
        Box::new(Self {
            arc_name: [0; 1024],
            file_name: [0; 1024],
            flags: 0,
            pack_size: 0,
            pack_size_high: 0,
            unp_size: 0,
            unp_size_high: 0,
            host_os: 0,
            file_crc: 0,
            file_time: 0,
            unp_ver: 0,
            method: 0,
            file_attr: 0,
            cmt_buf: null_mut(),
            cmt_buf_size: 0,
            cmt_size: 0,
            cmt_state: 0,
            reserved: [0; 1024],
        })
    }
}

/// `PackDefaultParamStruct`.
#[allow(
    dead_code,
    reason = "the C layout needs every field; the host reads only some"
)]
#[repr(C)]
struct PackDefaultParamStruct {
    size: i32,
    version_low: u32,
    version_high: u32,
    default_ini_name: [u8; 260],
}

type OpenArchiveW = unsafe extern "system" fn(*mut OpenArchiveDataW) -> Handle;
type ReadHeaderExW = unsafe extern "system" fn(Handle, *mut HeaderDataExW) -> i32;
type ProcessFileW = unsafe extern "system" fn(Handle, i32, *mut u16, *mut u16) -> i32;
type CloseArchive = unsafe extern "system" fn(Handle) -> i32;
type Callback = unsafe extern "system" fn(*mut u16, i32) -> i32;
type SetCallback = unsafe extern "system" fn(Handle, Callback);
type PackSetDefaultParams = unsafe extern "system" fn(*mut PackDefaultParamStruct);

/// `tChangeVolProcW`: no next volume is offered.
extern "system" fn change_volume(_name: *mut u16, _mode: i32) -> i32 {
    0
}

/// `tProcessDataProcW`: never cancel.
extern "system" fn process_data(_name: *mut u16, _size: i32) -> i32 {
    1
}

/// The packer functions of a loaded DLL.
pub struct Packer {
    open_archive: OpenArchiveW,
    read_header: ReadHeaderExW,
    process: ProcessFileW,
    close: CloseArchive,
    set_change_volume: Option<SetCallback>,
    set_process_data: Option<SetCallback>,
}

impl Packer {
    /// Binds the functions; `PackSetDefaultParams` is called once when the DLL has it.
    pub fn bind(library: &Library, ini: Option<[u8; 260]>) -> Option<Self> {
        let packer = Self {
            open_archive: bind!(library, "OpenArchiveW", OpenArchiveW)?,
            read_header: bind!(library, "ReadHeaderExW", ReadHeaderExW)?,
            process: bind!(library, "ProcessFileW", ProcessFileW)?,
            close: bind!(library, "CloseArchive", CloseArchive)?,
            set_change_volume: bind!(library, "SetChangeVolProcW", SetCallback),
            set_process_data: bind!(library, "SetProcessDataProcW", SetCallback),
        };
        if let (Some(defaults), Some(ini)) = (
            bind!(library, "PackSetDefaultParams", PackSetDefaultParams),
            ini,
        ) {
            let mut params = PackDefaultParamStruct {
                size: size_of::<PackDefaultParamStruct>() as i32,
                version_low: WCX_VERSION.1,
                version_high: WCX_VERSION.0,
                default_ini_name: ini,
            };
            // SAFETY: `params` is a `PackDefaultParamStruct` with its size set, alive for the call.
            unsafe { defaults(&mut params) };
        }
        Some(packer)
    }

    /// Opens `archive` in `mode`, with the callbacks set; closed when the guard drops.
    fn open(&self, archive: &Path, mode: i32) -> Result<Open<'_>, PluginError> {
        let mut name = wide(archive.as_os_str());
        let mut data = OpenArchiveDataW {
            arc_name: name.as_mut_ptr(),
            open_mode: mode,
            open_result: 0,
            cmt_buf: null_mut(),
            cmt_buf_size: 0,
            cmt_size: 0,
            cmt_state: 0,
        };
        // SAFETY: `data` and the name it points at live for the call; the DLL copies the name.
        let handle = unsafe { (self.open_archive)(&mut data) };
        if handle.is_null() {
            return Err(failed("OpenArchiveW", data.open_result));
        }
        if let Some(set) = self.set_change_volume {
            // SAFETY: `handle` is open; the callback is a plain function that never unwinds.
            unsafe { set(handle, change_volume) };
        }
        if let Some(set) = self.set_process_data {
            // SAFETY: as above.
            unsafe { set(handle, process_data) };
        }
        Ok(Open {
            packer: self,
            handle,
        })
    }

    pub fn list(&self, archive: &Path) -> Result<Vec<Entry>, PluginError> {
        let open = self.open(archive, PK_OM_LIST)?;
        let mut entries = Vec::new();
        while let Some(header) = open.next_header()? {
            entries.push(Entry {
                name: from_wide(&header.file_name),
                size: size(header.unp_size, header.unp_size_high),
                packed: size(header.pack_size, header.pack_size_high),
                modified: dos_time(header.file_time),
                attributes: header.file_attr as u32,
            });
            open.process(PK_SKIP, None)?;
        }
        Ok(entries)
    }

    pub fn extract(&self, archive: &Path, entry: &str, target: &Path) -> Result<(), PluginError> {
        let open = self.open(archive, PK_OM_EXTRACT)?;
        while let Some(header) = open.next_header()? {
            if from_wide(&header.file_name) == entry {
                return open.process(PK_EXTRACT, Some(target));
            }
            open.process(PK_SKIP, None)?;
        }
        Err(PluginError::Failed(format!(
            "no entry {entry} in the archive"
        )))
    }
}

/// An open archive, closed on drop.
struct Open<'a> {
    packer: &'a Packer,
    handle: Handle,
}

impl Open<'_> {
    /// The next header, `None` at the end.
    fn next_header(&self) -> Result<Option<Box<HeaderDataExW>>, PluginError> {
        let mut header = HeaderDataExW::empty();
        // SAFETY: the handle is open; `header` is a zeroed `tHeaderDataExW` the DLL fills.
        let result = unsafe { (self.packer.read_header)(self.handle, &mut *header) };
        match result {
            0 => Ok(Some(header)),
            E_END_ARCHIVE => Ok(None),
            code => Err(failed("ReadHeaderExW", code)),
        }
    }

    /// Skips the current entry, or extracts it to `target` (TC's form: no `DestPath`, the full
    /// target in `DestName`).
    fn process(&self, operation: i32, target: Option<&Path>) -> Result<(), PluginError> {
        let mut name = target.map(|target| wide(target.as_os_str()));
        let name_ptr = name.as_mut().map_or(null_mut(), |name| name.as_mut_ptr());
        // SAFETY: the handle is open and a header was just read; `name` lives for the call.
        let result = unsafe { (self.packer.process)(self.handle, operation, null_mut(), name_ptr) };
        if result == 0 {
            Ok(())
        } else {
            Err(failed("ProcessFileW", result))
        }
    }
}

impl Drop for Open<'_> {
    fn drop(&mut self) {
        // SAFETY: the handle came from `OpenArchiveW` and is closed once.
        unsafe {
            (self.packer.close)(self.handle);
        }
    }
}

fn failed(function: &str, code: i32) -> PluginError {
    PluginError::Failed(format!("{function}: {}", error_name(code)))
}
