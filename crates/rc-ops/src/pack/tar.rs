//! Tar, plain or inside gzip, xz or bzip2 (zstd is `write.rs`'s pipe): GNU headers, so long
//! names and large files fit, with the modification time and the Unix permissions; links are
//! stored as links.

use std::{
    fs::File,
    io::{self, BufWriter, Write},
};

use rc_vfs::unix_seconds;
use tar::{Builder, EntryType, Header};

use super::{
    PackFormat,
    write::{Feed, Member, MemberKind},
};

/// Writes `members` as a tar, compressed as `format` says, into `out`; the file back.
pub(super) fn write_compressed(
    out: BufWriter<File>,
    format: PackFormat,
    level: u32,
    members: &[Member],
    feed: &Feed,
) -> io::Result<File> {
    let out = match format {
        PackFormat::TarGz => {
            let gzip = flate2::write::GzEncoder::new(out, flate2::Compression::new(level));
            write_plain(gzip, members, feed)?.finish()?
        }
        PackFormat::TarXz => {
            let options = lzma_rust2::XzOptions::with_preset(level);
            let xz = lzma_rust2::XzWriter::new(out, options)?;
            write_plain(xz, members, feed)?.finish()?
        }
        PackFormat::TarBz2 => {
            // bzip2 always compresses: its levels are block sizes from 1 to 9.
            let level = bzip2::Compression::new(level.clamp(1, 9));
            write_plain(bzip2::write::BzEncoder::new(out, level), members, feed)?.finish()?
        }
        _ => write_plain(out, members, feed)?,
    };
    out.into_inner().map_err(io::IntoInnerError::into_error)
}

/// Writes `members` as a plain tar into `out`; `out` back after the closing blocks.
pub(super) fn write_plain<W: Write>(out: W, members: &[Member], feed: &Feed) -> io::Result<W> {
    let mut builder = Builder::new(out);
    for member in members {
        let mut header = Header::new_gnu();
        header.set_mtime(member.modified.and_then(unix_seconds).unwrap_or(0));
        match &member.kind {
            MemberKind::Dir => {
                header.set_entry_type(EntryType::Directory);
                header.set_mode(member.mode.unwrap_or(0o755));
                header.set_size(0);
                builder.append_data(&mut header, format!("{}/", member.name), io::empty())?;
            }
            MemberKind::Link(target) => {
                header.set_entry_type(EntryType::Symlink);
                header.set_mode(member.mode.unwrap_or(0o777));
                header.set_size(0);
                builder.append_link(&mut header, &member.name, target)?;
            }
            MemberKind::File => {
                let default = if member.readonly { 0o444 } else { 0o644 };
                header.set_entry_type(EntryType::Regular);
                header.set_mode(member.mode.unwrap_or(default));
                header.set_size(member.size);
                let source = feed.open(member)?;
                builder.append_data(&mut header, &member.name, source)?;
            }
        }
    }
    builder.into_inner()
}
