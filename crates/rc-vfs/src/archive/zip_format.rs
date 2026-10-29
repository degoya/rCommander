//! Zip, zip64 included: the listing from the central directory, an entry read by its index.
//!
//! Names without the UTF-8 flag are CP437, as the zip specification says and DOS and Windows
//! wrote them, unless the settings name another code page. Encrypted entries (ZipCrypto and
//! AES) are listed and marked; reading them is `password_required`, or with a password given
//! `password_wrong` when it does not fit. A reading session reads the central directory once
//! and every entry asked of it by index ([`serve`]).

use std::{
    io::{Read, Seek},
    path::Path,
    time::SystemTime,
};

use chrono::{Local, NaiveDate, TimeZone};
use encoding_rs::Encoding;
use zip::{
    DateTime, HasZipMetadata, System, ZipArchive, extra_fields::ExtraField, read::ZipFile,
    result::ZipError,
};

use super::{
    ArchiveSettings,
    password::{ArchivePassword, judge},
    pipe::{Pumped, pump},
    safety::Budget,
    session::{Feed, Request},
    source::{Source, io_error},
    tree::{RawEntry, RawKind, file_time, permission_bits, readonly_mode},
};
use crate::{Result, VfsError, unix_time};

/// Bytes of a link target read at most.
const MAX_LINK: u64 = 4096;

/// Every entry of the zip.
pub(crate) fn scan(source: &Source, settings: &ArchiveSettings) -> Result<Vec<RawEntry>> {
    let path = source.path();
    let mut archive =
        ZipArchive::new(source.seekable()?).map_err(|error| zip_error(error, path))?;
    let legacy = settings.legacy_encoding();
    let mut budget = Budget::new(&settings.limits, path);
    let mut entries = Vec::with_capacity(archive.len().min(4096));
    for index in 0..archive.len() {
        let member = archive
            .by_index_raw(index)
            .map_err(|error| zip_error(error, path))?;
        budget.admit(member.size())?;
        entries.push(describe(&member, index as u64, legacy));
    }
    // A link's target is its content: read now, for the few links there are.
    for entry in &mut entries {
        if let RawKind::Symlink(target) = &mut entry.kind
            && !entry.encrypted
        {
            *target = link_target(&mut archive, entry.index).unwrap_or_default();
        }
    }
    Ok(entries)
}

/// Serves `first` and every request after it from one opened zip; an encrypted entry is read
/// with `password`.
pub(crate) fn serve(
    source: &Source,
    password: Option<&ArchivePassword>,
    first: Request,
    feed: &mut Feed,
) {
    let opened = source.seekable().and_then(|reader| {
        ZipArchive::new(reader).map_err(|error| zip_error(error, &first.wanted.path))
    });
    let mut archive = match opened {
        Ok(archive) => archive,
        Err(error) => return first.fail(error),
    };
    let mut request = first;
    loop {
        let outcome = read_member(&mut archive, &request, password);
        match feed.finish(request, outcome) {
            Some(next) => request = next,
            None => return,
        }
    }
}

/// Streams the entry `request` wants into its pipe.
fn read_member<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    request: &Request,
    password: Option<&ArchivePassword>,
) -> Result<Pumped> {
    let path = &request.wanted.path;
    let index = usize::try_from(request.wanted.index)
        .map_err(|_| VfsError::NotFound { path: path.clone() })?;
    let password = password.filter(|_| request.wanted.encrypted);
    let member = match password {
        Some(password) => archive.by_index_decrypt(index, password.bytes()),
        None => archive.by_index(index),
    };
    let mut member = member.map_err(|error| zip_error(error, path))?;
    pump(&mut member, request.wanted.size, &request.writer, path)
        .map_err(|error| judge(error, password, path))
}

fn describe<R: Read>(
    member: &ZipFile<'_, R>,
    index: u64,
    legacy: Option<&'static Encoding>,
) -> RawEntry {
    let name = decode_name(member, legacy);
    let kind = if name.ends_with('/') || name.ends_with('\\') {
        RawKind::Dir
    } else if member.is_symlink() {
        RawKind::Symlink(String::new())
    } else {
        RawKind::File
    };
    let metadata = member.get_metadata();
    // DOS attributes: bit 0 read-only, bit 1 hidden; meaningful only from a DOS-like system.
    let dos = if metadata.system == System::Dos {
        metadata.external_attributes
    } else {
        0
    };
    let mode = member.unix_mode().map(permission_bits);
    let mut entry = RawEntry::new(name, kind, index);
    entry.size = member.size();
    entry.packed = Some(member.compressed_size());
    entry.modified = modified(member);
    entry.mode = mode;
    entry.readonly = dos & 1 != 0 || readonly_mode(mode);
    entry.hidden = dos & 2 != 0;
    entry.encrypted = member.encrypted();
    entry
}

/// The name: UTF-8 when flagged (or given in a Unicode path field), else the legacy code page.
fn decode_name<R: Read>(member: &ZipFile<'_, R>, legacy: Option<&'static Encoding>) -> String {
    match legacy {
        Some(encoding) if !member.get_metadata().is_utf8 => encoding
            .decode_without_bom_handling(member.name_raw())
            .0
            .into_owned(),
        // CP437, which the zip reader decodes itself.
        _ => member.name().to_owned(),
    }
}

/// The modification time: the NTFS or the Unix extra field where present (both UTC), else the
/// DOS time, which is the local time of whoever packed it - read as this machine's.
fn modified<R: Read>(member: &ZipFile<'_, R>) -> Option<SystemTime> {
    let exact = member.extra_data_fields().find_map(|field| {
        if let ExtraField::Ntfs(ntfs) = field {
            return file_time(ntfs.mtime());
        }
        if let ExtraField::ExtendedTimestamp(stamp) = field {
            return stamp
                .mod_time()
                .and_then(|seconds| unix_time(u64::from(seconds)));
        }
        None
    });
    exact.or_else(|| member.last_modified().and_then(dos_time))
}

/// A DOS date and time, as local time.
pub(crate) fn dos_time(stamp: DateTime) -> Option<SystemTime> {
    let naive = NaiveDate::from_ymd_opt(
        i32::from(stamp.year()),
        u32::from(stamp.month()),
        u32::from(stamp.day()),
    )?
    .and_hms_opt(
        u32::from(stamp.hour()),
        u32::from(stamp.minute()),
        u32::from(stamp.second()),
    )?;
    let local = Local.from_local_datetime(&naive).earliest()?;
    Some(SystemTime::from(local))
}

fn link_target<R: Read + Seek>(archive: &mut ZipArchive<R>, index: u64) -> Option<String> {
    let mut member = archive.by_index(usize::try_from(index).ok()?).ok()?;
    let mut target = Vec::new();
    member
        .by_ref()
        .take(MAX_LINK)
        .read_to_end(&mut target)
        .ok()?;
    Some(String::from_utf8_lossy(&target).into_owned())
}

fn zip_error(error: ZipError, path: &Path) -> VfsError {
    let path = path.to_path_buf();
    match error {
        ZipError::Io(error) => io_error(error, &path),
        ZipError::InvalidArchive(reason) => VfsError::ArchiveDamaged {
            path,
            reason: reason.into_owned(),
        },
        ZipError::UnsupportedArchive(ZipError::PASSWORD_REQUIRED) => {
            VfsError::PasswordRequired { path }
        }
        ZipError::InvalidPassword => VfsError::PasswordWrong { path },
        ZipError::UnsupportedArchive(_) | ZipError::CompressionMethodNotSupported(_) => {
            VfsError::Unsupported {
                path,
                operation: "reading this kind of zip entry",
            }
        }
        ZipError::FileNotFound => VfsError::NotFound { path },
        other => VfsError::ArchiveDamaged {
            path,
            reason: other.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "zip_format_tests.rs"]
mod tests;
