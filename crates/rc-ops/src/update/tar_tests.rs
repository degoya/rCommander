use std::{fs, io::Read, path::Path, sync::Arc};

use rc_vfs::{ArchiveFs, ArchiveSettings, LocalFs, Vfs};
use tar::{Archive, Builder, EntryType, Header};

use super::super::{
    Kind,
    test_support::{job, member, payload, run},
};
use super::walk;

/// A name of more than 100 bytes: a GNU long-name header goes before its entry.
fn long_name() -> String {
    format!("{}/{}.txt", "d".repeat(80), "n".repeat(40))
}

fn header(size: u64) -> Header {
    let mut header = Header::new_gnu();
    header.set_entry_type(EntryType::Regular);
    header.set_mode(0o644);
    header.set_mtime(1_700_000_000);
    header.set_size(size);
    header
}

/// `a.txt`, a file with a long name, one with a PAX time, `c.txt`.
fn write_tar(path: &Path) {
    let mut builder = Builder::new(fs::File::create(path).expect("create"));
    for (name, bytes) in [
        ("a.txt".to_owned(), b"alpha".to_vec()),
        (long_name(), payload(700, 3)),
    ] {
        let mut header = header(bytes.len() as u64);
        builder
            .append_data(&mut header, &name, bytes.as_slice())
            .expect("append");
    }
    builder
        .append_pax_extensions([("mtime", b"1700000000.123456789".as_slice())])
        .expect("pax");
    let mut pax = header(4);
    builder
        .append_data(&mut pax, "pax.txt", b"time".as_slice())
        .expect("append");
    let mut last = header(5);
    builder
        .append_data(&mut last, "c.txt", b"gamma".as_slice())
        .expect("append");
    builder.finish().expect("finish");
}

/// The bytes of each entry as it lies in the tar, extension headers included, by name.
fn raw(path: &Path) -> Vec<(String, Vec<u8>)> {
    let bytes = fs::read(path).expect("read");
    walk(path)
        .expect("a tar")
        .into_iter()
        .map(|(old, span)| {
            let start = usize::try_from(span.start).expect("small");
            let end = usize::try_from(span.end).expect("small");
            (old.name, bytes[start..end].to_vec())
        })
        .collect()
}

/// Acceptance: adding and removing; the long name and the PAX time come through byte for byte.
#[tokio::test]
async fn adding_and_removing_keeps_long_names_and_pax_times() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.tar");
    write_tar(&old);
    let before = raw(&old);
    assert_eq!(before.len(), 4);
    let new = dir.path().join("new.bin");
    fs::write(&new, payload(3000, 5)).expect("write");
    let update = job(Kind::Tar, &old, &[0], vec![member(&new, "sub/new.bin")]);
    let temp = update.temp.clone();
    let stats = run(update).await.expect("the update");
    assert_eq!((stats.raw, stats.decoded, stats.entries), (3, 0, 4));
    let after = raw(&temp);
    assert_eq!(after[..3], before[1..], "kept byte for byte");
    let mut archive = Archive::new(fs::File::open(&temp).expect("open"));
    let mut seen = Vec::new();
    for entry in archive.entries().expect("entries") {
        let mut entry = entry.expect("an entry");
        let name = String::from_utf8_lossy(&entry.path_bytes()).into_owned();
        if name == "pax.txt" {
            let mtime = entry
                .pax_extensions()
                .expect("pax")
                .expect("its extensions")
                .filter_map(Result::ok)
                .find(|extension| extension.key() == Ok("mtime"))
                .map(|extension| extension.value_bytes().to_vec());
            assert_eq!(mtime.as_deref(), Some(b"1700000000.123456789".as_slice()));
        }
        if name == "sub/new.bin" {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("read");
            assert!(bytes == payload(3000, 5), "the new entry reads back");
        }
        seen.push(name);
    }
    assert_eq!(
        seen,
        [
            long_name(),
            "pax.txt".into(),
            "c.txt".into(),
            "sub/new.bin".into()
        ]
    );
}

/// The positions the update removes by are those the archive backend lists.
#[tokio::test]
async fn positions_are_those_the_archive_backend_lists() {
    let dir = tempfile::tempdir().expect("tempdir");
    let old = dir.path().join("a.tar");
    write_tar(&old);
    let outer: Arc<dyn Vfs> = Arc::new(LocalFs::new());
    let fs = ArchiveFs::open(outer, &old, ArchiveSettings::default())
        .await
        .expect("open")
        .expect("a tar");
    for (old, _) in walk(&old).expect("a tar") {
        let listed = fs
            .entry_indices(&Path::new("/").join(&old.name))
            .await
            .expect("listed");
        assert_eq!(listed, [old.index], "{}", old.name);
    }
}
