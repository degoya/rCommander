//! The part size of a split: as typed - bytes, or a number with K, M or G, counted in KiB, MiB
//! and GiB as Total Commander counts them - and the sizes the split dialog offers.

use std::fmt;

use crate::{format, i18n::t};

const KIB: u64 = 1024;
const MIB: u64 = KIB * 1024;
const GIB: u64 = MIB * 1024;

/// A size the split dialog offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizePreset {
    Mib100,
    Gib1,
    /// The largest file FAT32 holds: 4 GiB less one byte.
    Fat32,
    /// A single-layer DVD-R: 4,700,372,992 bytes.
    Dvd,
}

impl SizePreset {
    pub const ALL: [Self; 4] = [Self::Mib100, Self::Gib1, Self::Fat32, Self::Dvd];

    #[must_use]
    pub fn bytes(self) -> u64 {
        match self {
            Self::Mib100 => 100 * MIB,
            Self::Gib1 => GIB,
            Self::Fat32 => 4 * GIB - 1,
            Self::Dvd => 4_700_372_992,
        }
    }

    /// What the size field shows once the size is chosen; [`parse_size`] reads it back.
    #[must_use]
    pub fn text(self) -> String {
        match self {
            Self::Mib100 => "100 MB".to_owned(),
            Self::Gib1 => "1 GB".to_owned(),
            Self::Fat32 | Self::Dvd => self.bytes().to_string(),
        }
    }
}

impl fmt::Display for SizePreset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Mib100 => t!("checksum.split.preset_100mb"),
            Self::Gib1 => t!("checksum.split.preset_1gb"),
            Self::Fat32 => t!("checksum.split.preset_fat32"),
            Self::Dvd => t!("checksum.split.preset_dvd"),
        })
    }
}

/// The part size `text` names ([`format::parse_size`]): `4294967295`, `700 MB`, `1.5G`,
/// `100 KiB`. `None` for anything else, and for less than one byte.
#[must_use]
pub fn parse_size(text: &str) -> Option<u64> {
    format::parse_size(text).filter(|bytes| *bytes > 0)
}

#[cfg(test)]
#[path = "size_tests.rs"]
mod tests;
