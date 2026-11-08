use std::{io, path::Path};

use rc_vfs::{ArchiveFormat, Compression, VfsError};

use super::{
    Kind, Refused, UpdateSettings, file_time, needed_space, normalize, ops_error, prefix, refused,
};
use crate::pack::write::panicked;

/// Compressed tars and single compressed files are not changed, as Total Commander refuses
/// them; zip, 7z and a plain tar are.
#[test]
fn only_zip_7z_and_a_plain_tar_are_changed() {
    assert_eq!(Kind::of(ArchiveFormat::Zip), Some(Kind::Zip));
    assert_eq!(Kind::of(ArchiveFormat::SevenZ), Some(Kind::SevenZ));
    assert_eq!(
        Kind::of(ArchiveFormat::Tar { compression: None }),
        Some(Kind::Tar)
    );
    for compression in [
        Compression::Gzip,
        Compression::Xz,
        Compression::Bzip2,
        Compression::Zstd,
    ] {
        let tar = ArchiveFormat::Tar {
            compression: Some(compression),
        };
        assert_eq!(Kind::of(tar), None, "{compression:?}");
        assert_eq!(Kind::of(ArchiveFormat::Single { compression }), None);
    }
}

#[test]
fn stored_names_compare_without_their_decoration() {
    assert_eq!(normalize("./a/b/"), "a/b");
    assert_eq!(normalize("/a\\b.txt"), "a/b.txt");
    assert_eq!(normalize("a.txt"), "a.txt");
    assert_eq!(normalize("/"), "");
}

/// The folder inside the archive: plain names only, `/` and empty for the root.
#[test]
fn the_folder_inside_is_plain_names() {
    assert_eq!(prefix("").expect("root"), "");
    assert_eq!(prefix("/").expect("root"), "");
    assert_eq!(prefix("/sub/dir").expect("plain"), "sub/dir/");
    for bad in ["../up", "a/../b", "a/con", "a/b:c"] {
        let error = prefix(bad).expect_err(bad);
        assert_eq!(error.code(), "invalid_name", "{bad}");
    }
}

/// The room needed: the old archive and every new file whole beside it, and a MiB more.
#[test]
fn the_room_needed_is_the_old_archive_and_the_new_files() {
    assert_eq!(needed_space(10, 20), 30 + (1 << 20));
    assert_eq!(needed_space(u64::MAX, 1), u64::MAX);
}

#[test]
fn refusals_become_their_codes() {
    let at = Path::new("/a.zip");
    let code = |error: io::Error| ops_error(error, at).code();
    assert_eq!(
        code(refused(Refused::NotModifiable)),
        "archive_not_modifiable"
    );
    assert_eq!(code(refused(Refused::Encrypted)), "archive_encrypted");
    assert_eq!(code(refused(Refused::Unverified)), "verify_failed");
    assert_eq!(code(panicked()), "pack_failed");
    let locked = VfsError::PasswordRequired { path: at.into() }.into_io();
    assert_eq!(code(locked), "archive_encrypted");
    let memory = VfsError::ArchiveDecoderMemory { path: at.into() }.into_io();
    assert_eq!(code(memory), "archive_decoder_memory");
    assert_eq!(
        code(io::Error::from(io::ErrorKind::StorageFull)),
        "disk_full"
    );
}

#[test]
fn a_windows_time_reads_back() {
    assert_eq!(file_time(0), None);
    let ticks = 116_444_736_000_000_000 + 10_000_000;
    assert_eq!(
        file_time(ticks),
        Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))
    );
}

/// The checked update is the default, as Total Commander's `VerifyZIP=1`.
#[test]
fn the_defaults_check_the_new_archive() {
    let settings = UpdateSettings::default();
    assert!(settings.verify && settings.recurse && settings.keep_paths);
    assert!(!settings.delete_sources);
    let parsed: UpdateSettings = serde_json::from_str("{}").expect("defaults");
    assert_eq!(parsed, settings);
}
