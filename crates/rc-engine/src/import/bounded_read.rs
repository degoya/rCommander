//! The one reader of "a regular file of at most N bytes" (RC-080-47): every file a person or an
//! agent names for the engine to take in whole - an ssh_config and its includes, Total
//! Commander's INI files, a Remote Desktop Manager export, a settings export, a plugin package.
//!
//! A folder, a device or a pipe is never read: the path is judged before it is opened (opening a
//! pipe blocks) and the file again as opened, so one swapped meanwhile is judged as it is. A file
//! larger than the limit is refused from its size, and read through `take`, so one that grows
//! while it is read cannot exceed it either. The buffer is zeroized when dropped: some of these
//! files hold passwords, obfuscated or not.

use std::{
    fs::{self, File},
    io::{self, Read as _},
    path::Path,
};

use zeroize::Zeroizing;

/// Why a file was not read.
#[derive(Debug, thiserror::Error)]
pub enum BoundedReadError {
    #[error(transparent)]
    Io(#[from] io::Error),
    /// A folder, a device or a pipe.
    #[error("not a regular file")]
    NotAFile,
    /// More than the limit's bytes.
    #[error("larger than {max} bytes")]
    TooLarge { max: u64 },
}

impl From<BoundedReadError> for io::Error {
    /// The failure as an I/O error: [`io::ErrorKind::InvalidInput`] for no regular file,
    /// [`io::ErrorKind::FileTooLarge`] for too large a file.
    fn from(error: BoundedReadError) -> Self {
        match error {
            BoundedReadError::Io(error) => error,
            BoundedReadError::NotAFile => {
                io::Error::new(io::ErrorKind::InvalidInput, "not a regular file")
            }
            BoundedReadError::TooLarge { max } => io::Error::new(
                io::ErrorKind::FileTooLarge,
                format!("larger than {max} bytes"),
            ),
        }
    }
}

/// The bytes of the regular file at `path`, when it holds at most `max`. Blocking file I/O.
///
/// # Errors
///
/// The file cannot be read, is no regular file, or holds more than `max` bytes.
pub(crate) fn read_bounded(path: &Path, max: u64) -> Result<Zeroizing<Vec<u8>>, BoundedReadError> {
    if !fs::metadata(path)?.is_file() {
        return Err(BoundedReadError::NotAFile);
    }
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(BoundedReadError::NotAFile);
    }
    if metadata.len() > max {
        return Err(BoundedReadError::TooLarge { max });
    }
    // Room for the whole file and the read that finds its end: the buffer is not grown, so no
    // copy of it is left behind unzeroized.
    let capacity = usize::try_from(metadata.len())
        .unwrap_or(0)
        .saturating_add(1);
    let mut bytes = Zeroizing::new(Vec::with_capacity(capacity));
    file.take(max.saturating_add(1)).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > max {
        return Err(BoundedReadError::TooLarge { max });
    }
    Ok(bytes)
}

#[cfg(test)]
#[path = "bounded_read_tests.rs"]
mod tests;
