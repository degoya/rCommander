//! The hash functions and the streaming hasher every operation of this crate reads through.

use std::{fmt::Write as _, path::Path, sync::Arc};

use rc_vfs::{ReadStream, Vfs};
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use tokio::io::AsyncReadExt as _;

use crate::{ChecksumError, Control};

/// The read buffer of every streamed hash, split and combine: the only memory a file of any
/// size costs.
pub const BUFFER_SIZE: usize = 256 * 1024;

/// A hash function. The table of what rCommander offers; a new one is a variant here and a
/// row in each `match` below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Algorithm {
    /// CRC32 (IEEE), what SFV files and Total Commander's split `.crc` files carry.
    Crc32,
    Md5,
    Sha1,
    Sha256,
    Sha512,
    Blake3,
}

impl Algorithm {
    /// Every algorithm, in the order a dialog offers them.
    pub const ALL: [Self; 6] = [
        Self::Crc32,
        Self::Md5,
        Self::Sha1,
        Self::Sha256,
        Self::Sha512,
        Self::Blake3,
    ];

    /// The extension of a checksum file of this algorithm, as Total Commander writes it.
    #[must_use]
    pub fn extension(self) -> &'static str {
        match self {
            Self::Crc32 => "sfv",
            Self::Md5 => "md5",
            Self::Sha1 => "sha",
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
            Self::Blake3 => "blake3",
        }
    }

    /// The algorithm a checksum file extension names (`sha1` and `b3` too), ignoring case.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "sfv" => Some(Self::Crc32),
            "md5" => Some(Self::Md5),
            "sha" | "sha1" => Some(Self::Sha1),
            "sha256" => Some(Self::Sha256),
            "sha512" => Some(Self::Sha512),
            "blake3" | "b3" => Some(Self::Blake3),
            _ => None,
        }
    }

    /// The algorithm a checksum file's name names by its extension ([`Self::from_extension`]):
    /// `set.sfv`, `disc.SHA256`; `None` for any other name. Enter checks such a file
    /// (RC-020-10).
    #[must_use]
    pub fn of_file_name(name: &str) -> Option<Self> {
        let (_, extension) = name.rsplit_once('.')?;
        Self::from_extension(extension)
    }

    /// The name in a BSD-style line (`SHA256 (name) = hex`), ignoring case.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag.to_ascii_uppercase().as_str() {
            "CRC32" => Some(Self::Crc32),
            "MD5" => Some(Self::Md5),
            "SHA1" => Some(Self::Sha1),
            "SHA256" => Some(Self::Sha256),
            "SHA512" => Some(Self::Sha512),
            "BLAKE3" => Some(Self::Blake3),
            _ => None,
        }
    }

    /// Hex digits of a digest.
    #[must_use]
    pub fn hex_len(self) -> usize {
        match self {
            Self::Crc32 => 8,
            Self::Md5 => 32,
            Self::Sha1 => 40,
            Self::Sha256 | Self::Blake3 => 64,
            Self::Sha512 => 128,
        }
    }

    /// The algorithm of a bare digest of `len` hex digits; SHA-256 for 64, which BLAKE3 shares
    /// (only an extension or a tag tells them apart).
    #[must_use]
    pub fn from_hex_len(len: usize) -> Option<Self> {
        match len {
            8 => Some(Self::Crc32),
            32 => Some(Self::Md5),
            40 => Some(Self::Sha1),
            64 => Some(Self::Sha256),
            128 => Some(Self::Sha512),
            _ => None,
        }
    }

    /// A fresh hasher.
    #[must_use]
    pub fn hasher(self) -> Hasher {
        match self {
            Self::Crc32 => Hasher::Crc32(crc32fast::Hasher::new()),
            Self::Md5 => Hasher::Md5(md5::Md5::new()),
            Self::Sha1 => Hasher::Sha1(sha1::Sha1::new()),
            Self::Sha256 => Hasher::Sha256(sha2::Sha256::new()),
            Self::Sha512 => Hasher::Sha512(sha2::Sha512::new()),
            Self::Blake3 => Hasher::Blake3(Box::new(blake3::Hasher::new())),
        }
    }
}

/// One running hash.
#[derive(Clone)]
pub enum Hasher {
    Crc32(crc32fast::Hasher),
    Md5(md5::Md5),
    Sha1(sha1::Sha1),
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
    /// Boxed: its state is far larger than the others'.
    Blake3(Box<blake3::Hasher>),
}

impl Hasher {
    /// Feeds `data`.
    pub fn update(&mut self, data: &[u8]) {
        match self {
            Self::Crc32(hasher) => hasher.update(data),
            Self::Md5(hasher) => hasher.update(data),
            Self::Sha1(hasher) => hasher.update(data),
            Self::Sha256(hasher) => hasher.update(data),
            Self::Sha512(hasher) => hasher.update(data),
            Self::Blake3(hasher) => {
                hasher.update(data);
            }
        }
    }

    /// The digest as lowercase hex.
    #[must_use]
    pub fn finish_hex(self) -> String {
        match self {
            Self::Crc32(hasher) => format!("{:08x}", hasher.finalize()),
            Self::Md5(hasher) => to_hex(&hasher.finalize()),
            Self::Sha1(hasher) => to_hex(&hasher.finalize()),
            Self::Sha256(hasher) => to_hex(&hasher.finalize()),
            Self::Sha512(hasher) => to_hex(&hasher.finalize()),
            Self::Blake3(hasher) => to_hex(hasher.finalize().as_bytes()),
        }
    }
}

/// `bytes` as lowercase hex.
fn to_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(hex, "{byte:02x}");
    }
    hex
}

/// Hashes everything `reader` yields through one buffer of [`BUFFER_SIZE`], whatever its
/// length; counts the bytes into `control`, waits while it is paused and stops with
/// `Cancelled` when it is cancelled.
///
/// # Errors
///
/// `Cancelled`, or the read failed (reported on `path`).
pub async fn hash_stream(
    mut reader: ReadStream,
    algorithm: Algorithm,
    path: &Path,
    control: &Control,
) -> Result<String, ChecksumError> {
    let mut hasher = algorithm.hasher();
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    loop {
        control.checkpoint().await?;
        let read = reader
            .read(&mut buffer)
            .await
            .map_err(|source| ChecksumError::io(path, source))?;
        if read == 0 {
            return Ok(hasher.finish_hex());
        }
        hasher.update(&buffer[..read]);
        control.advance(read as u64);
    }
}

/// Hashes the file at `path` on `vfs`, streamed.
///
/// # Errors
///
/// The file cannot be opened or read, the run's guard refuses it (`sensitive_path`), or the run
/// was cancelled.
pub async fn hash_file(
    vfs: &Arc<dyn Vfs>,
    path: &Path,
    algorithm: Algorithm,
    control: &Control,
) -> Result<String, ChecksumError> {
    control.admit(path).await?;
    let reader = vfs.open_read(path).await?;
    hash_stream(reader, algorithm, path, control).await
}

#[cfg(test)]
#[path = "algorithm_tests.rs"]
mod tests;
