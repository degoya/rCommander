use std::{
    path::Path,
    time::{Duration, UNIX_EPOCH},
};

use rc_vfs::{Attributes, BackendId, Entry, EntryKind, VfsPath};

use super::{FileMark, is_kept, unix_nanos};
use crate::JobKind;

fn entry(size: u64, secs: u64) -> Entry {
    Entry {
        name: "big.iso".to_owned(),
        kind: EntryKind::File,
        size,
        modified: Some(UNIX_EPOCH + Duration::from_secs(secs)),
        created: None,
        attributes: Attributes::default(),
        hidden: false,
        owner: None,
    }
}

fn server(path: &str) -> VfsPath {
    VfsPath {
        backend: BackendId::Sftp { connection_id: 7 },
        path: path.into(),
    }
}

#[test]
fn a_mark_holds_while_size_and_time_hold() {
    let mark = FileMark::new(
        Path::new("/srv/big.iso"),
        &entry(100, 50),
        Path::new("/down/big.iso"),
        false,
    );
    assert!(mark.is_of(Path::new("/srv/big.iso"), Path::new("/down/big.iso")));
    assert!(!mark.is_of(Path::new("/srv/other.iso"), Path::new("/down/big.iso")));
    assert!(mark.unchanged(&entry(100, 50)));
    assert!(!mark.unchanged(&entry(101, 50)), "another size");
    assert!(!mark.unchanged(&entry(100, 51)), "another time");
}

#[test]
fn a_mark_reads_back_from_json() {
    let mark = FileMark::new(Path::new("/a"), &entry(3, 9), Path::new("/b"), true);
    let json = serde_json::to_string(&mark).expect("json");
    assert_eq!(serde_json::from_str::<FileMark>(&json).expect("read"), mark);
    assert_eq!(mark.modified_ns, Some(9_000_000_000));
    assert_eq!(unix_nanos(None), None);
}

#[test]
fn transfers_to_or_from_a_server_and_from_disk_to_disk_are_kept() {
    let upload = JobKind::Copy {
        sources: vec![VfsPath::local("/home/a")],
        target: server("/up"),
    };
    let download = JobKind::MoveAs {
        source: server("/srv/a"),
        target: VfsPath::local("/home/b"),
    };
    // RC-080-04: a copy on this machine's disks survives a killed process too.
    let local = JobKind::Copy {
        sources: vec![VfsPath::local("/a")],
        target: VfsPath::local("/b"),
    };
    let out_of_an_archive = JobKind::Copy {
        sources: vec![VfsPath::archive(
            VfsPath::local("/a.zip"),
            rc_vfs::ArchiveFormat::Zip,
            "/inner.txt",
        )],
        target: VfsPath::local("/b"),
    };
    let delete = JobKind::Delete {
        sources: vec![server("/srv/a")],
        mode: crate::DeleteMode::Permanent,
    };
    assert!(is_kept(&upload));
    assert!(is_kept(&download));
    assert!(is_kept(&local));
    assert!(!is_kept(&out_of_an_archive));
    assert!(!is_kept(&delete));
}
