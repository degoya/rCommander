//! Junctions: a directory reparse point leading to another local directory, what `mklink /J`
//! makes. The buffer is built portably (`reparse.rs`); setting it is `DeviceIoControl`, the one
//! Win32 call here without a safe wrapper.

use std::{
    fs, io,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};

use windows_sys::Win32::System::IO::DeviceIoControl;

use super::super::reparse::mount_point_buffer;

/// `FSCTL_SET_REPARSE_POINT`.
const FSCTL_SET_REPARSE_POINT: u32 = 0x0009_00A4;
/// `GENERIC_WRITE`.
const GENERIC_WRITE: u32 = 0x4000_0000;
/// `FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS`: the directory itself.
const OPEN_DIRECTORY_ITSELF: u32 = 0x0020_0000 | 0x0200_0000;

/// A new directory at `real` that leads to `target`; removed again when the link fails.
pub(super) fn create(real: &Path, target: &Path) -> io::Result<()> {
    let absolute = std::path::absolute(target)?;
    let target = absolute
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "not valid Unicode"))?;
    let buffer = mount_point_buffer(target)?;
    fs::create_dir(real)?;
    let set = set_reparse_point(real, &buffer);
    if set.is_err() {
        // The empty directory made above; the error that matters is the one returned.
        let _ = fs::remove_dir(real);
    }
    set
}

#[allow(
    unsafe_code,
    reason = "DeviceIoControl has no safe wrapper in std, winsafe or windows-sys: it reads \
              `buffer` for its stated length and writes only `returned`, both live for the \
              call, on a handle the File keeps open until after it"
)]
fn set_reparse_point(dir: &Path, buffer: &[u8]) -> io::Result<()> {
    let handle = fs::OpenOptions::new()
        .access_mode(GENERIC_WRITE)
        .custom_flags(OPEN_DIRECTORY_ITSELF)
        .open(dir)?;
    let length =
        u32::try_from(buffer.len()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut returned = 0u32;
    // SAFETY: the handle is open for the whole call; the input pointer and length describe
    // `buffer`, there is no output buffer, `returned` is a valid u32 and no OVERLAPPED is used
    // (the handle is synchronous).
    let done = unsafe {
        DeviceIoControl(
            handle.as_raw_handle(),
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            length,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if done == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
