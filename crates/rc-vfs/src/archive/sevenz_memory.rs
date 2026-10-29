//! The decoder memory a 7z asks for, checked before sevenz-rust2 decodes anything (RC-080-35).
//!
//! sevenz-rust2 0.23 has no memory limit a caller can set (its `MAX_MEM_LIMIT_KB` is
//! `usize::MAX / 1024`), and its LZMA reader checks none at all: the dictionary a coder's
//! properties state is set aside when the first byte is decoded. So the coders are read here
//! first - those of an encoded header from the raw header bytes, before `Archive::read`
//! decodes it; those of a content block from the parsed archive, before the block is read - and
//! a dictionary over [`ArchiveLimits::max_decoder_memory`] is `archive_decoder_memory`.

use std::{
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

use sevenz_rust2::{Block, EncoderMethod};

use super::ArchiveLimits;
use crate::{Result, VfsError};

/// The signature header: signature, version, start header CRC, next header offset, size, CRC.
const SIGNATURE_HEADER: u64 = 32;
const SIGNATURE: &[u8] = b"7z\xBC\xAF\x27\x1C";
/// How far from the end sevenz-rust2 looks for a header when the start header is blank.
const SEARCH_TAIL: u64 = 1 << 20;

const K_END: u8 = 0x00;
const K_PACK_INFO: u8 = 0x06;
const K_UNPACK_INFO: u8 = 0x07;
const K_SIZE: u8 = 0x09;
const K_CRC: u8 = 0x0A;
const K_FOLDER: u8 = 0x0B;
const K_ENCODED_HEADER: u8 = 0x17;

/// The bytes one coder sets aside: the dictionary of LZMA and LZMA2, the model of PPMd; the
/// other methods keep small fixed buffers.
pub(crate) fn coder_memory(method: &[u8], properties: &[u8]) -> u64 {
    let stated = || {
        let bytes: [u8; 4] = properties.get(1..5)?.try_into().ok()?;
        Some(u64::from(u32::from_le_bytes(bytes)))
    };
    if method == EncoderMethod::ID_LZMA || method == EncoderMethod::ID_PPMD {
        return stated().unwrap_or(0);
    }
    if method == EncoderMethod::ID_LZMA2 {
        return match properties.first().copied() {
            Some(40) => 0xFFFF_FFFF,
            Some(bits) if bits < 40 => u64::from(2 | (bits & 1)) << (bits / 2 + 11),
            // sevenz-rust2 refuses the coder itself.
            _ => 0,
        };
    }
    0
}

/// The bytes a chain of coders (method id, properties) sets aside together.
fn block_memory<'a>(coders: impl Iterator<Item = (&'a [u8], &'a [u8])>) -> u64 {
    coders.fold(0_u64, |sum, (method, properties)| {
        sum.saturating_add(coder_memory(method, properties))
    })
}

/// Checked before `block` is decoded.
pub(crate) fn check_block(block: &Block, limits: &ArchiveLimits, path: &Path) -> Result<()> {
    let coders = block
        .coders
        .iter()
        .map(|coder| (coder.encoder_method_id(), coder.properties()));
    limits.check_decoder_memory(block_memory(coders), path)
}

/// Checked before `Archive::read`: the coders of an encoded header, found where sevenz-rust2
/// will look for it. A file that is no 7z, or has a plain header, is left to sevenz-rust2.
pub(crate) fn check_header<R: Read + Seek>(
    reader: &mut R,
    limits: &ArchiveLimits,
    path: &Path,
) -> Result<()> {
    let checked = |coders: Vec<(Vec<u8>, Vec<u8>)>| {
        let coders = coders.iter().map(|(m, p)| (m.as_slice(), p.as_slice()));
        limits.check_decoder_memory(block_memory(coders), path)
    };
    let failed = |error: io::Error| super::source::io_error(error, path);
    let length = reader.seek(SeekFrom::End(0)).map_err(failed)?;
    reader.seek(SeekFrom::Start(0)).map_err(failed)?;
    let mut start = Vec::with_capacity(SIGNATURE_HEADER as usize);
    reader
        .by_ref()
        .take(SIGNATURE_HEADER)
        .read_to_end(&mut start)
        .map_err(failed)?;
    if start.len() < SIGNATURE_HEADER as usize || !start.starts_with(SIGNATURE) {
        return Ok(());
    }
    if start[8..].iter().all(|&byte| byte == 0) {
        // A blank start header (an archive left unfinished): sevenz-rust2 tries every header
        // byte near the end, so every one is checked.
        let from = length.saturating_sub(SEARCH_TAIL).max(SIGNATURE_HEADER);
        reader.seek(SeekFrom::Start(from)).map_err(failed)?;
        let mut tail = Vec::new();
        reader.read_to_end(&mut tail).map_err(failed)?;
        for at in (0..tail.len()).rev() {
            if let Some(coders) = encoded_header_coders(&tail[at..]) {
                checked(coders)?;
            }
        }
        return Ok(());
    }
    let number = |range: std::ops::Range<usize>| {
        start
            .get(range)
            .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
            .map(u64::from_le_bytes)
    };
    let (Some(offset), Some(size)) = (number(12..20), number(20..28)) else {
        return Ok(());
    };
    let Some(at) = SIGNATURE_HEADER
        .checked_add(offset)
        .filter(|at| *at < length)
    else {
        return Ok(());
    };
    reader.seek(SeekFrom::Start(at)).map_err(failed)?;
    let mut header = Vec::new();
    // sevenz-rust2 reads the whole next header too; only an encoded one is read on here.
    reader
        .by_ref()
        .take(1)
        .read_to_end(&mut header)
        .map_err(failed)?;
    if header.first() != Some(&K_ENCODED_HEADER) {
        return Ok(());
    }
    reader
        .take(size.min(length - at).saturating_sub(1))
        .read_to_end(&mut header)
        .map_err(failed)?;
    // An encoded header this does not read, sevenz-rust2 does not either: refused, so a header
    // read differently can never slip past the check.
    let coders = encoded_header_coders(&header).ok_or_else(|| VfsError::ArchiveDamaged {
        path: path.to_path_buf(),
        reason: "the encoded header does not read".to_owned(),
    })?;
    checked(coders)
}

/// The coders (method id, properties) of the first folder of the encoded header `bytes`
/// starts with - the only folder sevenz-rust2 decodes it with; `None` when it does not parse.
fn encoded_header_coders(bytes: &[u8]) -> Option<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut header = Cursor { bytes, at: 0 };
    if header.byte()? != K_ENCODED_HEADER {
        return None;
    }
    let mut nid = header.byte()?;
    if nid == K_PACK_INFO {
        header.number()?;
        let streams = header.number()?;
        nid = header.byte()?;
        if nid == K_SIZE {
            for _ in 0..streams {
                header.number()?;
            }
            nid = header.byte()?;
        }
        if nid == K_CRC {
            let defined = header.defined(streams)?;
            header.skip(defined.checked_mul(4)?)?;
            nid = header.byte()?;
        }
        if nid != K_END {
            return None;
        }
        nid = header.byte()?;
    }
    if nid != K_UNPACK_INFO || header.byte()? != K_FOLDER {
        return None;
    }
    // At least one folder, not external.
    if header.number()? == 0 || header.byte()? != 0 {
        return None;
    }
    let count = header.number()?;
    let mut coders = Vec::new();
    for _ in 0..count {
        let flags = header.byte()?;
        let method = header.take(usize::from(flags & 0x0F))?.to_vec();
        if flags & 0x10 != 0 {
            header.number()?;
            header.number()?;
        }
        let properties = if flags & 0x20 != 0 {
            let size = usize::try_from(header.number()?).ok()?;
            header.take(size)?.to_vec()
        } else {
            Vec::new()
        };
        coders.push((method, properties));
    }
    Some(coders)
}

/// A reader over header bytes; every step fails at their end.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let taken = self.bytes.get(self.at..self.at.checked_add(count)?)?;
        self.at += count;
        Some(taken)
    }

    fn skip(&mut self, count: u64) -> Option<()> {
        self.take(usize::try_from(count).ok()?).map(|_| ())
    }

    fn byte(&mut self) -> Option<u8> {
        self.take(1).map(|bytes| bytes[0])
    }

    /// 7z's variable-length number: the leading one bits of the first byte count the bytes
    /// that follow.
    fn number(&mut self) -> Option<u64> {
        let first = u64::from(self.byte()?);
        let mut mask = 0x80_u64;
        let mut value = 0_u64;
        for index in 0..8 {
            if first & mask == 0 {
                return Some(value | ((first & (mask - 1)) << (8 * index)));
            }
            value |= u64::from(self.byte()?) << (8 * index);
            mask >>= 1;
        }
        Some(value)
    }

    /// How many of `count` items an "all defined, or a bit vector" field marks.
    fn defined(&mut self, count: u64) -> Option<u64> {
        if self.byte()? != 0 {
            return Some(count);
        }
        let bits = self.take(usize::try_from(count.div_ceil(8)).ok()?)?;
        // The first bit is the high bit of the first byte; the padding after `count` is not read.
        let ones = (0..bits.len() * 8)
            .take(usize::try_from(count).ok()?)
            .filter(|&bit| bits[bit / 8] & (0x80 >> (bit % 8)) != 0)
            .count();
        u64::try_from(ones).ok()
    }
}

#[cfg(test)]
#[path = "sevenz_memory_tests.rs"]
mod tests;
