//! The Mark-of-the-Web (RC-110-56): the `Zone.Identifier` stream Windows keeps beside a file
//! that came from the internet, so SmartScreen asks before a program out of it starts and Office
//! opens a document in its protected view. A copy, a move, an unpacking and the temporary copy
//! Enter makes of a file in an archive carry it on:
//!
//! - a file on this machine passes its own stream on, byte for byte, as Windows' own copy does;
//! - a file out of an archive gets the archive's zone, at least the internet's (`ZoneId=3`), with
//!   its `ReferrerUrl` and `HostUrl`; inside an archive in an archive, the outermost one's;
//! - a file from a server (SFTP, FTP) gets the internet's zone with the server as `HostUrl`;
//! - a portable device's files carry none, as in Explorer.
//!
//! Which zone goes where is portable logic ([`origin`], [`carry`], [`to_write`]); reading and
//! writing the stream is Windows' only (`zone/windows.rs`, a path `<file>:Zone.Identifier`
//! opened with `std::fs`) - elsewhere there is no stream and both do nothing. A stream that
//! cannot be written (FAT, a share without streams) costs the mark, never the copy: the job
//! reports it once (`zone_not_kept`).

use std::{
    io,
    path::{Path, PathBuf},
};

use rc_vfs::BackendId;

#[cfg(windows)]
mod windows;

/// Whether this platform keeps zones at all; elsewhere callers skip the work.
pub const SUPPORTED: bool = cfg!(windows);

/// The longest stream passed on as it is. A longer one is no zone a browser wrote: the copy gets
/// the internet's zone instead.
pub const MAX_STREAM: u64 = 64 * 1024;

/// `URLZONE_INTERNET`.
pub const INTERNET: u32 = 3;

/// A zone as the stream states it; the keys rCommander carries on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    /// `ZoneId`: 0 this machine, 1 the intranet, 2 trusted sites, 3 the internet, 4 restricted.
    pub id: u32,
    /// `ReferrerUrl`: the page the download started from.
    pub referrer_url: Option<String>,
    /// `HostUrl`: where the file came from.
    pub host_url: Option<String>,
}

impl Zone {
    /// The internet's zone, with `host_url` as where the file came from.
    #[must_use]
    pub fn internet(host_url: Option<&str>) -> Self {
        Self {
            id: INTERNET,
            referrer_url: None,
            host_url: host_url.and_then(clean_url),
        }
    }

    /// The zone a stream's bytes state. A stream that is there but names no zone that reads is
    /// the internet's: a damaged mark never lowers the guard.
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Self {
        let text = String::from_utf8_lossy(bytes);
        let mut zone = Self::internet(None);
        let mut id = None;
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if key.eq_ignore_ascii_case("ZoneId") {
                id = value.parse().ok();
            } else if key.eq_ignore_ascii_case("ReferrerUrl") {
                zone.referrer_url = clean_url(value);
            } else if key.eq_ignore_ascii_case("HostUrl") {
                zone.host_url = clean_url(value);
            }
        }
        zone.id = id.unwrap_or(INTERNET);
        zone
    }

    /// The zone of an entry unpacked from an archive with this zone: the same, at least the
    /// internet's.
    #[must_use]
    pub fn unpacked(self) -> Self {
        Self {
            id: self.id.max(INTERNET),
            ..self
        }
    }

    /// The stream's text, as Windows writes it.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = format!("[ZoneTransfer]\r\nZoneId={}\r\n", self.id);
        if let Some(url) = &self.referrer_url {
            text.push_str(&format!("ReferrerUrl={url}\r\n"));
        }
        if let Some(url) = &self.host_url {
            text.push_str(&format!("HostUrl={url}\r\n"));
        }
        text
    }
}

/// A URL fit for one line of the stream: not empty, no line break or other control character.
fn clean_url(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && !value.chars().any(char::is_control)).then(|| value.to_owned())
}

/// Where the zone of the files read from a backend comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A file on this machine: its own stream.
    OwnStream,
    /// Out of the archive file at this path on this machine: the archive's zone, at least the
    /// internet's.
    Archive(PathBuf),
    /// From a server: the internet's zone, the server as `HostUrl`.
    Server(BackendId),
    /// Nothing to carry on: a portable device, a test's memory.
    Unmarked,
}

/// Where the zone of what is read from `backend` comes from. An archive takes its origin from
/// where its file lies: on this machine its own stream, on a server the server, inside another
/// archive that one's.
#[must_use]
pub fn origin(backend: &BackendId) -> Origin {
    match backend {
        BackendId::Local => Origin::OwnStream,
        BackendId::Archive { outer, .. } => match origin(&outer.backend) {
            Origin::OwnStream => Origin::Archive(outer.path.clone()),
            other => other,
        },
        server if server.is_server() => Origin::Server(server.clone()),
        _ => Origin::Unmarked,
    }
}

/// What every file of one origin carries on, found once per source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Carry {
    /// Each file its own stream.
    OwnStream,
    /// This zone, for every file.
    Fixed(Zone),
    /// None.
    Nothing,
}

/// What the files of `origin` carry on; `host_url` names a server. Blocking: reads an archive's
/// stream.
#[must_use]
pub fn carry(origin: &Origin, host_url: Option<&str>) -> Carry {
    carry_with(origin, host_url, read)
}

/// [`carry`] with `read` for the stream of an archive file.
pub(crate) fn carry_with(
    origin: &Origin,
    host_url: Option<&str>,
    read: impl FnOnce(&Path) -> io::Result<Option<Vec<u8>>>,
) -> Carry {
    match origin {
        Origin::OwnStream => Carry::OwnStream,
        Origin::Archive(archive) => match read(archive) {
            Ok(Some(bytes)) => Carry::Fixed(Zone::parse(&bytes).unpacked()),
            Ok(None) => Carry::Nothing,
            Err(error) => {
                tracing::debug!(%error, "an archive's zone could not be read");
                Carry::Nothing
            }
        },
        Origin::Server(_) => Carry::Fixed(Zone::internet(host_url)),
        Origin::Unmarked => Carry::Nothing,
    }
}

/// The stream a copy gets: `None` for none. `source_stream` reads the source file's own one,
/// only for [`Carry::OwnStream`]; one too long to be a zone gives the internet's.
pub(crate) fn to_write(
    carry: &Carry,
    source_stream: impl FnOnce() -> io::Result<Option<Vec<u8>>>,
) -> Option<Vec<u8>> {
    match carry {
        Carry::Nothing => None,
        Carry::Fixed(zone) => Some(zone.render().into_bytes()),
        Carry::OwnStream => match source_stream() {
            Ok(Some(bytes)) if u64::try_from(bytes.len()).is_ok_and(|len| len <= MAX_STREAM) => {
                Some(bytes)
            }
            Ok(Some(_)) => Some(Zone::internet(None).render().into_bytes()),
            Ok(None) => None,
            // A source whose stream cannot be read (FAT) has none to pass on.
            Err(error) => {
                tracing::debug!(%error, "a file's zone could not be read");
                None
            }
        },
    }
}

/// Gives `dest`, copied from `source`, the zone `carry` says. Blocking.
///
/// # Errors
///
/// The stream could not be written; the copy itself stands.
pub fn keep(carry: &Carry, source: &Path, dest: &Path) -> io::Result<()> {
    match to_write(carry, || read(source)) {
        Some(bytes) => write(dest, &bytes),
        None => Ok(()),
    }
}

/// Marks `file` with `zone`. Blocking.
///
/// # Errors
///
/// The stream could not be written.
pub fn mark(file: &Path, zone: &Zone) -> io::Result<()> {
    write(file, zone.render().as_bytes())
}

/// The bytes of `file`'s zone stream, at most [`MAX_STREAM`] and one; `None` without one.
///
/// # Errors
///
/// The stream is there but cannot be read.
#[cfg(windows)]
pub fn read(file: &Path) -> io::Result<Option<Vec<u8>>> {
    windows::read(file)
}

/// No streams outside Windows.
///
/// # Errors
///
/// None.
#[cfg(not(windows))]
pub fn read(_file: &Path) -> io::Result<Option<Vec<u8>>> {
    Ok(None)
}

/// Writes `bytes` as `file`'s zone stream, replacing one there.
///
/// # Errors
///
/// The file system has no streams (FAT), or refuses the write.
#[cfg(windows)]
pub fn write(file: &Path, bytes: &[u8]) -> io::Result<()> {
    windows::write(file, bytes)
}

/// No streams outside Windows: nothing is written.
///
/// # Errors
///
/// None.
#[cfg(not(windows))]
pub fn write(_file: &Path, _bytes: &[u8]) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[path = "zone_tests.rs"]
mod tests;
