//! 7z: the listing from the header, an entry streamed out of its block.
//!
//! In a solid archive the entries of a block are one compressed stream; reading the n-th means
//! unpacking the ones before it. They are unpacked to nowhere and the target is streamed through
//! the pipe, so neither the block nor the entry is ever held in memory. A reading session
//! ([`serve`]) reads the header once and walks a block on while the reads go forward in it, so
//! the entries of a solid block read in order unpack it once. An archive whose header is
//! encrypted cannot be listed without a password; one whose content is encrypted lists, with
//! every entry of an encrypted block marked.

use std::{
    io::{self, Read},
    path::Path,
};

use sevenz_rust2::{
    Archive, ArchiveEntry, BlockDecoder, EncoderMethod, Error as SevenError, Password,
};

use super::{
    ArchiveLimits, ArchiveSettings,
    password::{ArchivePassword, judge},
    pipe::pump,
    safety::Budget,
    session::{Feed, Request, not_found},
    sevenz_memory,
    source::{SeekReader, Source, io_error},
    tree::{RawEntry, RawKind, file_time, permission_bits, readonly_mode},
};
use crate::{Result, VfsError};

/// The p7zip extension: the high 16 bits of the attributes hold a Unix mode.
const UNIX_EXTENSION: u32 = 0x8000;

/// Every entry of the archive.
pub(crate) fn scan(
    source: &Source,
    settings: &ArchiveSettings,
    password: Option<&ArchivePassword>,
) -> Result<Vec<RawEntry>> {
    let path = source.path();
    let mut reader = source.seekable()?;
    sevenz_memory::check_header(&mut reader, &settings.limits, path)?;
    let key = password.map_or_else(Password::empty, ArchivePassword::sevenz);
    let archive = Archive::read(&mut reader, &key)
        .map_err(|error| seven_error(error, path, password.is_some()))?;
    let blocks = blocks(&archive);
    let mut budget = Budget::new(&settings.limits, path);
    let mut entries = Vec::with_capacity(archive.files.len().min(4096));
    for (index, file) in archive.files.iter().enumerate() {
        if file.is_anti_item {
            // A deletion marker of an update, not an entry.
            continue;
        }
        budget.admit(file.size)?;
        let block = archive
            .stream_map
            .file_block_index
            .get(index)
            .copied()
            .flatten()
            .and_then(|block| blocks.get(block));
        let mut entry = describe(file, index as u64);
        entry.packed = block.map(|block| block.share(file.size));
        entry.encrypted = block.is_some_and(|block| block.encrypted);
        entries.push(entry);
    }
    Ok(entries)
}

/// The archive a session reads: only the entries with a stream (see [`streams_only`]), and
/// where each entry of the full archive is in it.
struct Streams {
    archive: Archive,
    /// Per entry of the full archive: its block and its place in `archive`; `None` without a
    /// stream (an empty file).
    places: Vec<Option<(usize, usize)>>,
}

/// Serves `first` and every request after it from one opened 7z; an entry of an encrypted
/// block, or of an archive with an encrypted header, with `password`. Neither the header nor a
/// block is decoded when its dictionary passes the limits' decoder memory.
pub(crate) fn serve(
    source: &Source,
    limits: &ArchiveLimits,
    password: Option<&ArchivePassword>,
    first: Request,
    feed: &mut Feed,
) {
    let key = password.map_or_else(Password::empty, ArchivePassword::sevenz);
    let (mut reader, streams) = match open(source, limits, &key, password, &first.wanted.path) {
        Ok(opened) => opened,
        Err(error) => return first.fail(error),
    };
    let mut pending = Some(first);
    while let Some(request) = pending.take() {
        let path = &request.wanted.path;
        let place = match streams.place(request.wanted.index, path) {
            Ok(place) => place,
            Err(error) => return request.fail(error),
        };
        let Some((block, target)) = place else {
            // No stream: an empty file.
            let outcome = pump(&mut io::empty(), request.wanted.size, &request.writer, path);
            pending = feed.finish(request, outcome);
            continue;
        };
        if let Some(coders) = streams.archive.blocks.get(block)
            && let Err(error) = sevenz_memory::check_block(coders, limits, path)
        {
            return request.fail(error);
        }
        let walk = Walk {
            streams: &streams,
            key: &key,
            password,
            block,
        };
        pending = walk.run(&mut reader, (request, target), feed);
    }
}

/// Reads the header, within the limits' decoder memory.
fn open(
    source: &Source,
    limits: &ArchiveLimits,
    key: &Password,
    password: Option<&ArchivePassword>,
    path: &Path,
) -> Result<(SeekReader, Streams)> {
    let mut reader = source.seekable()?;
    sevenz_memory::check_header(&mut reader, limits, path)?;
    let archive = Archive::read(&mut reader, key)
        .map_err(|error| seven_error(error, path, password.is_some()))?;
    Ok((reader, streams_only(archive)))
}

/// One walk through a block, from its start.
struct Walk<'a> {
    streams: &'a Streams,
    key: &'a Password,
    password: Option<&'a ArchivePassword>,
    block: usize,
}

impl Walk<'_> {
    /// Serves `first` (with its place in the block) and every next request further on in the
    /// block; answers the request that lies elsewhere, to be walked to from another start, and
    /// `None` once the producer ends.
    fn run(
        &self,
        reader: &mut SeekReader,
        first: (Request, usize),
        feed: &mut Feed,
    ) -> Option<Request> {
        let archive = &self.streams.archive;
        let given = self.password;
        let mut at = archive
            .stream_map
            .block_first_file_index
            .get(self.block)
            .copied()
            .unwrap_or(0);
        let mut current = Some(first);
        let mut elsewhere = None;
        let decoder = BlockDecoder::new(1, self.block, archive, self.key, reader);
        let finished =
            decoder.for_each_entries(&mut |entry: &ArchiveEntry, data: &mut dyn Read| {
                let here = at;
                at += 1;
                let Some((_, target)) = &current else {
                    return Ok(false);
                };
                if here < *target {
                    // An entry before the target in a solid block: unpacked to nowhere.
                    io::copy(data, &mut io::sink()).map_err(SevenError::from)?;
                    return Ok(true);
                }
                if here > *target || !std::ptr::eq(entry, &archive.files[*target]) {
                    // Not where it should be: not found, below.
                    return Ok(false);
                }
                let Some((request, _)) = current.take() else {
                    return Ok(false);
                };
                let path = &request.wanted.path;
                let outcome = pump(data, request.wanted.size, &request.writer, path)
                    .map_err(|error| judge(error, given, path));
                let Some(next) = feed.finish(request, outcome) else {
                    return Ok(false);
                };
                match self.streams.place(next.wanted.index, &next.wanted.path) {
                    Ok(Some((block, target))) if block == self.block && target > here => {
                        current = Some((next, target));
                        Ok(true)
                    }
                    _ => {
                        elsewhere = Some(next);
                        Ok(false)
                    }
                }
            });
        if let Some((request, _)) = current {
            let path = &request.wanted.path;
            let error = match finished {
                Err(error) => seven_error(error, path, given.is_some()),
                Ok(_) => not_found(path),
            };
            let error = judge(error, given, path);
            request.fail(error);
        }
        elsewhere
    }
}

impl Streams {
    /// The block of entry `index` of the full archive and its place in [`Self::archive`]; `None`
    /// for an entry without a stream.
    fn place(&self, index: u64, path: &Path) -> Result<Option<(usize, usize)>> {
        let index = usize::try_from(index).map_err(|_| not_found(path))?;
        self.places
            .get(index)
            .copied()
            .ok_or_else(|| not_found(path))
    }
}

/// `BlockDecoder` (sevenz-rust2 0.23) walks `num_unpack_sub_streams` entries from a block's first
/// file and counts the folders among them (the library gives them a block too), so it stops
/// short of the block's last streams when a folder lies in between. A copy of the archive that
/// keeps only the entries with a stream walks exactly the block's streams.
fn streams_only(mut archive: Archive) -> Streams {
    let blocks = std::mem::take(&mut archive.stream_map.file_block_index);
    let files = std::mem::take(&mut archive.files);
    let mut places = vec![None; files.len()];
    let mut kept_blocks = Vec::new();
    let mut kept_files = Vec::new();
    for (index, file) in files.into_iter().enumerate() {
        if !file.has_stream {
            continue;
        }
        let block = blocks.get(index).copied().flatten();
        places[index] = block.map(|block| (block, kept_files.len()));
        kept_blocks.push(block);
        kept_files.push(file);
    }
    let mut first_seen = vec![false; archive.stream_map.block_first_file_index.len()];
    for (position, owner) in kept_blocks.iter().enumerate() {
        if let Some(block) = *owner
            && let Some(seen) = first_seen.get_mut(block)
            && !*seen
        {
            *seen = true;
            archive.stream_map.block_first_file_index[block] = position;
        }
    }
    archive.files = kept_files;
    archive.stream_map.file_block_index = kept_blocks;
    Streams { archive, places }
}

fn describe(file: &ArchiveEntry, index: u64) -> RawEntry {
    let attributes = if file.has_windows_attributes {
        file.windows_attributes
    } else {
        0
    };
    let unix = (attributes & UNIX_EXTENSION != 0).then_some(attributes >> 16);
    let kind = if file.is_directory {
        RawKind::Dir
    } else if unix.is_some_and(|mode| mode & 0o170_000 == 0o120_000) {
        // The target is the entry's content, inside a compressed block: not unpacked to list.
        RawKind::Symlink(String::new())
    } else {
        RawKind::File
    };
    let mode = unix.map(permission_bits);
    let mut entry = RawEntry::new(file.name.clone(), kind, index);
    entry.size = file.size;
    entry.modified = if file.has_last_modified_date {
        file_time(u64::from(file.last_modified_date))
    } else {
        None
    };
    entry.mode = mode;
    // Windows attributes: bit 0 read-only, bit 1 hidden.
    entry.readonly = attributes & 1 != 0 || readonly_mode(mode);
    entry.hidden = attributes & 2 != 0;
    entry
}

/// What the listing needs of a block.
struct BlockFacts {
    encrypted: bool,
    packed: u64,
    unpacked: u64,
}

impl BlockFacts {
    /// An entry's share of the block's packed bytes, so its ratio is the block's: the bytes of
    /// one entry of a solid block are not told apart.
    fn share(&self, size: u64) -> u64 {
        if self.unpacked == 0 {
            return self.packed;
        }
        let share = u128::from(self.packed) * u128::from(size) / u128::from(self.unpacked);
        u64::try_from(share).unwrap_or(u64::MAX)
    }
}

fn blocks(archive: &Archive) -> Vec<BlockFacts> {
    let first_packs = archive.stream_map.block_first_pack_stream_index();
    archive
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| BlockFacts {
            encrypted: block
                .coders
                .iter()
                .any(|coder| coder.encoder_method_id() == EncoderMethod::ID_AES256_SHA256),
            packed: first_packs
                .get(index)
                .and_then(|&first| archive.pack_sizes().get(first))
                .copied()
                .unwrap_or(0),
            unpacked: block.get_unpack_size(),
        })
        .collect()
}

/// `with_password`: a password was given, so a failing key is a wrong one, not a missing one -
/// also when the data it decrypted reads as damaged (an encrypted header, say).
fn seven_error(error: SevenError, path: &Path, with_password: bool) -> VfsError {
    let mapped = seven_error_kind(error, path, with_password);
    match mapped {
        VfsError::ArchiveDamaged { path, .. } if with_password => VfsError::PasswordWrong { path },
        other => other,
    }
}

fn seven_error_kind(error: SevenError, path: &Path, with_password: bool) -> VfsError {
    let path = path.to_path_buf();
    match error {
        SevenError::PasswordRequired | SevenError::MaybeBadPassword(_) if with_password => {
            VfsError::PasswordWrong { path }
        }
        SevenError::PasswordRequired | SevenError::MaybeBadPassword(_) => {
            VfsError::PasswordRequired { path }
        }
        SevenError::Io(error, _) | SevenError::FileOpen(error, _) => io_error(error, &path),
        SevenError::UnsupportedCompressionMethod(_)
        | SevenError::Unsupported(_)
        | SevenError::ExternalUnsupported => VfsError::Unsupported {
            path,
            operation: "reading this 7z compression method",
        },
        SevenError::MaxMemLimited { .. } => VfsError::ArchiveDecoderMemory { path },
        other => VfsError::ArchiveDamaged {
            path,
            reason: other.to_string(),
        },
    }
}

#[cfg(test)]
#[path = "sevenz_format_tests.rs"]
mod tests;
