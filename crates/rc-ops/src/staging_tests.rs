use std::path::Path;

use rc_vfs::BackendId;

use super::*;

#[test]
fn a_part_file_sits_beside_its_target() {
    assert_eq!(
        part_path(Path::new("/srv/data/report.pdf")),
        Path::new("/srv/data/report.pdf.rcpart")
    );
    assert_eq!(part_path(Path::new("/a")), Path::new("/a.rcpart"));
}

#[test]
fn only_servers_are_remote() {
    assert!(is_remote(&BackendId::Sftp { connection_id: 3 }));
    assert!(is_remote(&BackendId::Ftp { connection_id: 3 }));
    assert!(!is_remote(&BackendId::Local));
    assert!(!is_remote(&BackendId::Memory(1)));
}

/// RC-080-37: a file out of an archive goes through a part onto a disk or a server, but starts
/// over instead of resuming; into memory it is written in place.
#[test]
fn staged_and_resumable_are_apart() {
    let archive = BackendId::Archive {
        outer: Box::new(rc_vfs::VfsPath::local("/a.zip")),
        format: rc_vfs::ArchiveFormat::Zip,
    };
    let server = BackendId::Sftp { connection_id: 3 };
    assert!(is_staged(&BackendId::Local));
    assert!(is_staged(&server));
    assert!(!is_staged(&BackendId::Memory(1)));
    assert!(!is_resumable(&archive, &BackendId::Local));
    assert!(is_resumable(&archive, &server));
    assert!(is_resumable(&BackendId::Local, &BackendId::Local));
    assert!(is_resumable(&server, &BackendId::Memory(1)));
    assert!(!is_resumable(&BackendId::Memory(1), &BackendId::Memory(2)));
}

/// A file of this machine keeps its whole mode; a server's, a device's or an archive's loses
/// set-uid, set-gid, sticky and everyone's write.
#[test]
fn a_foreign_source_keeps_no_special_bits_and_no_write_for_everyone() {
    assert_eq!(kept_mode(0o4755, &BackendId::Local), 0o4755);
    for src in [
        BackendId::Sftp { connection_id: 1 },
        BackendId::Ftp { connection_id: 1 },
    ] {
        assert_eq!(kept_mode(0o4755, &src), 0o755, "{src:?}");
        assert_eq!(kept_mode(0o2775, &src), 0o775, "{src:?}");
        assert_eq!(kept_mode(0o1777, &src), 0o775, "{src:?}");
        assert_eq!(kept_mode(0o666, &src), 0o664, "{src:?}");
        assert_eq!(kept_mode(0o600, &src), 0o600, "{src:?}");
    }
}
