//! What keeps a malicious archive harmless: entry names cleaned into paths that stay inside the
//! archive, and the limits that stop an archive bomb.
//!
//! A name is never joined to a real path. One that climbs out (`..`), is absolute, starts with a
//! drive letter or a UNC prefix is listed under its cleaned path and marked with the
//! [`UnsafeName`] it showed; backslashes are separators, as Windows reads them.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{Result, VfsError, drive_letter};

/// Why a stored entry name was not safe to use as a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnsafeName {
    /// A `..` component: the name climbs out of the archive (zip slip).
    ParentDir,
    /// A leading `/` or `\`.
    Absolute,
    /// A drive letter (`C:`).
    DriveLetter,
    /// A UNC prefix (`\\server\share`, `//server/share`).
    Unc,
}

/// [`ArchiveLimits::max_decoder_memory`] unless configured: 256 MiB, more than every preset of
/// xz and 7-Zip asks for (64 MiB at most).
const DEFAULT_DECODER_MEMORY: u64 = 256 << 20;

/// The limits against archive bombs, with safe defaults; the settings may raise them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ArchiveLimits {
    /// Entries an archive may have; more refuse the listing.
    pub max_entries: u64,
    /// Bytes all entries may unpack to together; more refuse the listing, and a compressed
    /// stream stops unpacking there.
    pub max_unpacked_size: u64,
    /// Unpacked bytes per packed byte an entry (or a compressed stream) may reach once it is
    /// larger than `ratio_threshold`.
    pub max_ratio: u64,
    /// Below this many unpacked bytes the ratio is not checked: a small run of zeros is harmless.
    pub ratio_threshold: u64,
    /// Bytes a decoder may set aside for its dictionary (xz, 7z's LZMA, LZMA2 and PPMd): a header
    /// that asks for more is refused before anything is reserved (RC-080-35).
    pub max_decoder_memory: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: 1_000_000,
            max_unpacked_size: 1 << 40,
            max_ratio: 500,
            ratio_threshold: 256 << 20,
            max_decoder_memory: DEFAULT_DECODER_MEMORY,
        }
    }
}

impl ArchiveLimits {
    /// Whether `unpacked` bytes out of `packed` pass the ratio.
    fn over_ratio(&self, unpacked: u64, packed: u64) -> bool {
        unpacked >= self.ratio_threshold && unpacked > packed.saturating_mul(self.max_ratio)
    }

    /// The decoder memory limit in KiB, as lzma-rust2 takes it; never `u32::MAX`, which turns
    /// its limit off.
    pub(crate) fn decoder_memory_kib(&self) -> u32 {
        u32::try_from(self.max_decoder_memory / 1024)
            .unwrap_or(u32::MAX)
            .min(u32::MAX - 1)
    }

    /// Checked before a decoder is made: the `needed` bytes its header asks for.
    pub(crate) fn check_decoder_memory(&self, needed: u64, path: &Path) -> Result<()> {
        if needed > self.max_decoder_memory {
            return Err(VfsError::ArchiveDecoderMemory {
                path: path.to_path_buf(),
            });
        }
        Ok(())
    }

    /// Checked before an entry is read: its stated size against its packed size, where the
    /// format knows it per entry (zip, 7z).
    pub(crate) fn check_entry(&self, size: u64, packed: Option<u64>, path: &Path) -> Result<()> {
        match packed {
            Some(packed) if self.over_ratio(size, packed) => Err(limit(path, "compression ratio")),
            _ => Ok(()),
        }
    }

    /// Checked while a compressed stream unpacks (tar.gz, a single .xz): the bytes out so far
    /// against the bytes in so far.
    pub(crate) fn check_stream(&self, unpacked: u64, packed: u64, path: &Path) -> Result<()> {
        if unpacked > self.max_unpacked_size {
            return Err(limit(path, "unpacked size"));
        }
        if self.over_ratio(unpacked, packed) {
            return Err(limit(path, "compression ratio"));
        }
        Ok(())
    }
}

fn limit(path: &Path, limit: &'static str) -> VfsError {
    VfsError::ArchiveLimit {
        path: path.to_path_buf(),
        limit,
    }
}

/// The entry count and the stated sizes of a listing, counted while it is read so a bomb stops
/// the scan instead of filling memory.
pub(crate) struct Budget<'a> {
    limits: &'a ArchiveLimits,
    path: &'a Path,
    entries: u64,
    unpacked: u64,
}

impl<'a> Budget<'a> {
    pub(crate) fn new(limits: &'a ArchiveLimits, path: &'a Path) -> Self {
        Self {
            limits,
            path,
            entries: 0,
            unpacked: 0,
        }
    }

    /// Counts one more entry of `size` stated bytes.
    pub(crate) fn admit(&mut self, size: u64) -> Result<()> {
        self.entries += 1;
        if self.entries > self.limits.max_entries {
            return Err(limit(self.path, "entry count"));
        }
        self.unpacked = self.unpacked.saturating_add(size);
        if self.unpacked > self.limits.max_unpacked_size {
            return Err(limit(self.path, "unpacked size"));
        }
        Ok(())
    }
}

/// A stored name as path components inside the archive, and what was wrong with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CleanName {
    /// The components, without empty ones, `.`, `..`, a root, a drive or a UNC prefix.
    pub parts: Vec<String>,
    /// The first unsafe trait the name showed; `None` for an ordinary name.
    pub issue: Option<UnsafeName>,
}

/// Cleans a stored name: `/` and `\` separate, `..` and every prefix are dropped and noted.
pub(crate) fn clean_name(name: &str) -> CleanName {
    let bytes = name.as_bytes();
    let separator = |byte: u8| byte == b'/' || byte == b'\\';
    let mut issue = None;
    let mut rest = name;
    if bytes.len() >= 2 && separator(bytes[0]) && separator(bytes[1]) {
        issue = Some(UnsafeName::Unc);
    } else if drive_letter(name).is_some() {
        issue = Some(UnsafeName::DriveLetter);
        // Both bytes are ASCII, so 2 is a character boundary.
        rest = &name[2..];
    } else if bytes.first().is_some_and(|&byte| separator(byte)) {
        issue = Some(UnsafeName::Absolute);
    }
    let mut parts = Vec::new();
    for part in rest.split(['/', '\\']) {
        match part {
            "" | "." => {}
            ".." => {
                if issue.is_none() {
                    issue = Some(UnsafeName::ParentDir);
                }
            }
            part => parts.push(part.to_owned()),
        }
    }
    CleanName { parts, issue }
}

#[cfg(test)]
#[path = "safety_tests.rs"]
mod tests;
