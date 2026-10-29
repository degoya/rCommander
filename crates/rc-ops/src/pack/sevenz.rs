//! 7z: LZMA2 with the level as its preset (0 stores), solid in one block or one block per file.
//!
//! A solid block reads its files one after the other through lazy sources, so only one is open
//! at a time. Empty files carry no stream, as 7-Zip writes them. Times go in as Windows times;
//! a Unix mode goes into the high half of the attributes with p7zip's flag, so 7-Zip, p7zip and
//! the archive backend read it back.

use std::{
    fs::File,
    io::{self, BufReader, BufWriter, Read},
};

use sevenz_rust2::{
    ArchiveEntry, ArchiveWriter, EncoderConfiguration, EncoderMethod, NtTime, SourceReader,
    encoder_options::Lzma2Options,
};

use super::write::{Feed, Member, MemberKind, Source};

/// p7zip's flag: the high 16 bits of the attributes hold a Unix mode.
const UNIX_EXTENSION: u32 = 0x8000;
/// Windows attributes: read-only, directory, archive.
const READONLY: u32 = 0x1;
const DIRECTORY: u32 = 0x10;
const ARCHIVE: u32 = 0x20;

/// Writes `members` (links are not among them) as a 7z into `out`; the file back.
pub(super) fn write(
    out: BufWriter<File>,
    members: &[Member],
    level: u32,
    solid: bool,
    feed: &Feed,
) -> io::Result<File> {
    let mut writer = ArchiveWriter::new(out).map_err(seven_error)?;
    let method = if level == 0 {
        EncoderConfiguration::new(EncoderMethod::COPY)
    } else {
        EncoderConfiguration::from(Lzma2Options::from_level(level))
    };
    writer.set_content_methods(vec![method]);
    let mut block: Vec<(ArchiveEntry, &Member)> = Vec::new();
    for member in members {
        let entry = entry(member);
        match member.kind {
            MemberKind::File if member.size > 0 && solid => block.push((entry, member)),
            MemberKind::File if member.size > 0 => {
                let source = feed.open(member)?;
                writer
                    .push_archive_entry(entry, Some(source))
                    .map_err(seven_error)?;
            }
            // Folders, empty files (and links, which never get here): no stream.
            _ => {
                writer
                    .push_archive_entry::<&[u8]>(entry, None)
                    .map_err(seven_error)?;
            }
        }
    }
    if !block.is_empty() {
        let (entries, readers): (Vec<ArchiveEntry>, Vec<SourceReader<Lazy<'_>>>) = block
            .into_iter()
            .map(|(entry, member)| {
                let lazy = Lazy {
                    feed,
                    member,
                    source: None,
                };
                (entry, SourceReader::new(lazy))
            })
            .unzip();
        writer
            .push_archive_entries(entries, readers)
            .map_err(seven_error)?;
    }
    writer
        .finish()?
        .into_inner()
        .map_err(io::IntoInnerError::into_error)
}

/// The entry of `member`, with its time and attributes.
fn entry(member: &Member) -> ArchiveEntry {
    let mut entry = match member.kind {
        MemberKind::Dir => ArchiveEntry::new_directory(&member.name),
        _ => ArchiveEntry::new_file(&member.name),
    };
    if let Some(date) = member.modified.and_then(|time| NtTime::try_from(time).ok()) {
        entry.last_modified_date = date;
        entry.has_last_modified_date = true;
    }
    let dir = member.kind == MemberKind::Dir;
    let mut attributes = if dir { DIRECTORY } else { ARCHIVE };
    if member.readonly {
        attributes |= READONLY;
    }
    if let Some(mode) = member.mode {
        let kind = if dir { 0o040_000 } else { 0o100_000 };
        attributes |= UNIX_EXTENSION | ((kind | (mode & 0o7777)) << 16);
    }
    entry.has_windows_attributes = true;
    entry.windows_attributes = attributes;
    entry
}

/// A source of a solid block, opened when the block reaches it.
struct Lazy<'a> {
    feed: &'a Feed,
    member: &'a Member,
    source: Option<BufReader<Source<'a>>>,
}

impl Read for Lazy<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.source.is_none() {
            self.source = Some(self.feed.open(self.member)?);
        }
        match &mut self.source {
            Some(source) => source.read(buf),
            None => Ok(0),
        }
    }
}

/// The 7z crate's error as an I/O error; one that wraps an I/O error (a source's, the disk's)
/// gives that back, so its code survives.
fn seven_error(error: sevenz_rust2::Error) -> io::Error {
    match error {
        sevenz_rust2::Error::Io(error, _) => error,
        other => io::Error::other(other),
    }
}
