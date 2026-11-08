//! 7z: the blocks that keep all their entries copied raw (`push_raw_block`: packed bytes,
//! coders, sizes and CRCs as they were), a block that loses an entry decoded - within the decoder
//! memory of the limits - and its other entries encoded again into one block, a block that loses
//! all of them dropped. Entries without data (folders, empty files) are written anew from their
//! headers; the new entries are packed as a pack packs them.

use std::{
    fs::File,
    io::{self, BufWriter, Read},
    path::Path,
};

use rc_vfs::{ArchiveLimits, VfsError, check_sevenz_block, check_sevenz_header};
use sevenz_rust2::{
    Archive, ArchiveEntry, ArchiveWriter, BlockDecoder, EncoderMethod, Error as SevenError,
    Password,
};

use super::{
    Old, Paced, Refused, Rewrite, Stats, file_time, normalize, open_old, refused, sevenz_pipe,
};
use crate::pack::{
    sevenz::{content_method, push_members, seven_error},
    write::Feed,
};

/// Every entry of the 7z; an encrypted header or block refuses the archive.
pub(super) fn scan(path: &Path, limits: &ArchiveLimits) -> io::Result<Vec<Old>> {
    let archive = read(path, limits)?.1;
    Ok(archive
        .files
        .iter()
        .enumerate()
        .filter(|(_, file)| !file.is_anti_item)
        .map(|(index, file)| Old {
            index: index as u64,
            name: normalize(&file.name),
            dir: file.is_directory,
            size: file.size,
            modified: file
                .has_last_modified_date
                .then(|| file_time(u64::from(file.last_modified_date)))
                .flatten(),
        })
        .collect())
}

/// The archive's header, read within the limits' decoder memory; encrypted ones refused.
fn read(path: &Path, limits: &ArchiveLimits) -> io::Result<(io::BufReader<File>, Archive)> {
    let mut reader = open_old(path)?;
    check_sevenz_header(&mut reader, limits, path).map_err(VfsError::into_io)?;
    let archive = Archive::read(&mut reader, &Password::empty()).map_err(|error| match error {
        SevenError::PasswordRequired | SevenError::MaybeBadPassword(_) => {
            refused(Refused::Encrypted)
        }
        other => seven_error(other),
    })?;
    let encrypted = archive.blocks.iter().any(|block| {
        block
            .coders
            .iter()
            .any(|coder| coder.encoder_method_id() == EncoderMethod::ID_AES256_SHA256)
    });
    if encrypted {
        return Err(refused(Refused::Encrypted));
    }
    Ok((reader, archive))
}

/// The entries with data of each block, in their order.
fn by_block(archive: &Archive) -> Vec<Vec<usize>> {
    let mut blocks = vec![Vec::new(); archive.blocks.len()];
    for (index, file) in archive.files.iter().enumerate() {
        let block = archive
            .stream_map
            .file_block_index
            .get(index)
            .copied()
            .flatten();
        if let Some(entries) = block
            .filter(|_| file.has_stream)
            .and_then(|b| blocks.get_mut(b))
        {
            entries.push(index);
        }
    }
    blocks
}

/// Writes the new 7z into `out`.
pub(super) fn rewrite(
    job: &Rewrite,
    feed: &Feed,
    out: BufWriter<File>,
) -> io::Result<(File, Stats)> {
    let (reader, archive) = read(&job.old, &job.limits)?;
    let mut source = Paced::new(reader, feed);
    let mut writer = ArchiveWriter::new(out).map_err(seven_error)?;
    writer.set_content_methods(vec![content_method(job.level)]);
    let removed = |index: usize| job.remove.contains(&(index as u64));
    let mut stats = Stats::default();
    // The blocks written so far: the new archive's number of the next one.
    let mut written = 0_usize;
    for (index, file) in archive.files.iter().enumerate() {
        if file.is_anti_item || file.has_stream || removed(index) {
            continue;
        }
        writer
            .push_archive_entry::<&[u8]>(file.clone(), None)
            .map_err(seven_error)?;
        stats.raw += 1;
    }
    for (block, entries) in by_block(&archive).iter().enumerate() {
        let kept: Vec<usize> = entries.iter().copied().filter(|&i| !removed(i)).collect();
        if kept.is_empty() {
            continue;
        }
        if kept.len() == entries.len() {
            writer
                .push_raw_block(&archive, block, &mut source, |entry| entry.name.clone())
                .map_err(seven_error)?;
            stats.raw += kept.len() as u64;
            stats.raw_blocks.push(written);
            written += 1;
            continue;
        }
        let coded = archive.blocks.get(block).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "a 7z block missing from its header",
            )
        })?;
        check_sevenz_block(coded, &job.limits, &job.old).map_err(VfsError::into_io)?;
        let again = sevenz_pipe::Again {
            archive: &archive,
            block,
            kept: &kept,
        };
        again.encode(&mut source, &mut writer)?;
        stats.decoded += kept.len() as u64;
        written += 1;
    }
    push_members(&mut writer, &job.members, job.solid, feed)?;
    stats.entries = stats.raw + stats.decoded + job.members.len() as u64;
    let file = writer
        .finish()?
        .into_inner()
        .map_err(io::IntoInnerError::into_error)?;
    Ok((file, stats))
}

/// Reads the new 7z back: as many entries as written, and every block that was encoded (not
/// copied raw) decoded through its CRCs.
pub(super) fn verify(job: &Rewrite, stats: &Stats) -> io::Result<()> {
    let (mut reader, archive) = read(&job.temp, &job.limits)?;
    let count = archive
        .files
        .iter()
        .filter(|file| !file.is_anti_item)
        .count();
    if count as u64 != stats.entries {
        return Err(refused(Refused::Unverified));
    }
    for block in 0..archive.blocks.len() {
        if stats.raw_blocks.contains(&block) {
            continue;
        }
        check_sevenz_block(&archive.blocks[block], &job.limits, &job.temp)
            .map_err(VfsError::into_io)?;
        let key = Password::empty();
        let decoder = BlockDecoder::new(1, block, &archive, &key, &mut reader);
        decoder
            .for_each_entries(&mut |_: &ArchiveEntry, data: &mut dyn Read| {
                io::copy(data, &mut io::sink()).map_err(SevenError::from)?;
                Ok(true)
            })
            .map_err(|_| refused(Refused::Unverified))?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "sevenz_tests.rs"]
mod tests;
