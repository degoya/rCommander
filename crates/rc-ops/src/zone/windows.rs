//! The zone stream on Windows: `<file>:Zone.Identifier`, an NTFS alternate data stream, opened
//! with `std::fs` like any file.

use std::{
    ffi::OsString,
    fs::File,
    io::{self, Read as _},
    path::Path,
};

/// The path of `file`'s zone stream.
fn stream(file: &Path) -> OsString {
    let mut stream = file.as_os_str().to_owned();
    stream.push(":Zone.Identifier");
    stream
}

/// The stream's bytes, at most [`super::MAX_STREAM`] and one; `None` when there is none.
pub(super) fn read(file: &Path) -> io::Result<Option<Vec<u8>>> {
    let opened = match File::open(stream(file)) {
        Ok(opened) => opened,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut bytes = Vec::new();
    opened
        .take(super::MAX_STREAM.saturating_add(1))
        .read_to_end(&mut bytes)?;
    Ok(Some(bytes))
}

/// Writes the stream, replacing one there.
pub(super) fn write(file: &Path, bytes: &[u8]) -> io::Result<()> {
    std::fs::write(stream(file), bytes)
}
