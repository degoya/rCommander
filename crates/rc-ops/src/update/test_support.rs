//! What the archive update's unit tests share: a rewrite run as the job runs it, its new
//! entries read from the local disk.

use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use rc_vfs::{ArchiveFs, ArchiveLimits, ArchiveSettings, LocalFs, Vfs};
use tokio::{io::AsyncReadExt, sync::watch};

use super::{Kind, Rewrite, Stats, rewrite};
use crate::{
    control::Control,
    pack::write::{Feed, Member, MemberKind},
};

/// `len` bytes that differ by `seed`.
pub(super) fn payload(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|at| (at % 251) as u8 ^ seed).collect()
}

/// The file `path` as a new entry named `name`.
pub(super) fn member(path: &Path, name: &str) -> Member {
    let size = std::fs::metadata(path).expect("a source").len();
    Member {
        path: path.to_path_buf(),
        name: name.to_owned(),
        kind: MemberKind::File,
        size,
        modified: None,
        mode: None,
        readonly: false,
    }
}

/// The rewrite of `old` without the entries `remove` and with `members`, checked after.
pub(super) fn job(kind: Kind, old: &Path, remove: &[u64], members: Vec<Member>) -> Rewrite {
    Rewrite {
        kind,
        old: old.to_path_buf(),
        temp: temp_of(old),
        remove: remove.iter().copied().collect(),
        members,
        level: 6,
        solid: true,
        limits: ArchiveLimits::default(),
        verify: true,
    }
}

/// Where a test's new archive is written: beside the old one.
pub(super) fn temp_of(old: &Path) -> PathBuf {
    old.with_file_name("new.rcpart")
}

/// Runs `job` as the job does: on a blocking thread, the new entries read through a backend.
pub(super) async fn run(job: Rewrite) -> io::Result<Stats> {
    let (bytes, _seen) = watch::channel(0_u64);
    let src: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    let feed = Feed::new(src, Control::default(), bytes)?;
    tokio::task::spawn_blocking(move || rewrite(&job, &feed))
        .await
        .expect("the rewrite does not panic")
}

/// Every file at the top of the archive `path`, by name, as the archive backend reads it.
pub(super) async fn files(path: &Path) -> Vec<(String, Vec<u8>)> {
    let outer: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    let fs = ArchiveFs::open(outer, path, ArchiveSettings::default())
        .await
        .expect("open")
        .expect("an archive");
    let mut all = Vec::new();
    for entry in fs.list(Path::new("/")).await.expect("list") {
        if entry.kind != rc_vfs::EntryKind::File {
            continue;
        }
        let mut stream = fs
            .open_read(&Path::new("/").join(&entry.name))
            .await
            .expect("read");
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.expect("read");
        all.push((entry.name, bytes));
    }
    all.sort();
    all
}
