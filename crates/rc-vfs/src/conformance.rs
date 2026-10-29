//! The backend conformance suite: what every [`Vfs`] owes its callers, written once and run by
//! each backend against its own fixture.
//!
//! A backend's tests call [`run`] with the backend and an empty, writable directory on it, and
//! [`copy_between`] with two backends (or two instances of one). The checks are test code: they
//! panic on the first difference, with a message that names what differed. Capabilities decide
//! what is checked - a backend that cannot set times is not asked to.
//!
//! A backend that cannot write (an archive) runs [`run_read_only`] against a tree prepared
//! beforehand - the one [`prepare_read_only`] writes - and [`writes_are_unsupported`].
//!
//! Compiled for this crate's own tests, and for other crates behind the `conformance` feature:
//! `rc-vfs = { path = "../rc-vfs", features = ["conformance"] }` under `[dev-dependencies]`.

use std::{
    io::SeekFrom,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};

use crate::{Entry, EntryKind, Vfs, WriteMode};

/// Writes `bytes` to `path` and closes the stream.
pub async fn write_file(vfs: &dyn Vfs, path: &Path, mode: WriteMode, bytes: &[u8]) {
    let mut stream = vfs
        .open_write(path, mode)
        .await
        .unwrap_or_else(|error| panic!("open {} for writing: {error}", path.display()));
    stream.write_all(bytes).await.expect("write");
    stream.shutdown().await.expect("shutdown after writing");
}

/// The whole contents of the file at `path`.
pub async fn read_file(vfs: &dyn Vfs, path: &Path) -> Vec<u8> {
    let mut stream = vfs
        .open_read(path)
        .await
        .unwrap_or_else(|error| panic!("open {} for reading: {error}", path.display()));
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).await.expect("read");
    bytes
}

/// The entries of `dir`, sorted by name.
async fn listing(vfs: &dyn Vfs, dir: &Path) -> Vec<Entry> {
    let mut entries = vfs
        .list(dir)
        .await
        .unwrap_or_else(|error| panic!("list {}: {error}", dir.display()));
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// The code of the error an operation must fail with.
fn code_of<T>(result: crate::Result<T>, what: &str) -> &'static str {
    match result {
        Ok(_) => panic!("{what}: succeeded, an error was expected"),
        Err(error) => error.code(),
    }
}

/// Every check, each in a directory of its own under `root`, which must exist and be empty.
pub async fn run(vfs: &dyn Vfs, root: &Path) {
    run_but_case(vfs, root).await;
    let case = root.join("case");
    vfs.mkdir(&case, false)
        .await
        .unwrap_or_else(|error| panic!("conformance case: mkdir: {error}"));
    case_follows_the_capability(vfs, &case).await;
}

/// Every check but [`case_follows_the_capability`], for a test server whose disk does not match
/// the case its backend claims (an SFTP server on APFS or NTFS).
pub async fn run_but_case(vfs: &dyn Vfs, root: &Path) {
    let dir = |name: &str| -> PathBuf { root.join(name) };
    let checks = [
        "write_modes",
        "listing",
        "stat",
        "typed_errors",
        "mkdir",
        "rename",
        "remove",
        "set_mtime",
        "free_space",
    ];
    for name in checks {
        vfs.mkdir(&dir(name), false)
            .await
            .unwrap_or_else(|error| panic!("conformance {name}: mkdir: {error}"));
    }
    write_modes(vfs, &dir("write_modes")).await;
    listing_describes_files_and_directories(vfs, &dir("listing")).await;
    stat_describes_one_entry(vfs, &dir("stat")).await;
    wrong_kinds_are_typed_errors(vfs, &dir("typed_errors")).await;
    mkdir_with_and_without_parents(vfs, &dir("mkdir")).await;
    rename_within_the_backend(vfs, &dir("rename")).await;
    remove_files_and_directories(vfs, &dir("remove")).await;
    set_mtime_is_what_stat_reads_back(vfs, &dir("set_mtime")).await;
    free_space_when_offered(vfs, &dir("free_space")).await;
}

/// `CreateNew` refuses an existing file, `Append` continues it, `open_read_at` reads from an
/// offset, `Truncate` replaces it.
pub async fn write_modes(vfs: &dyn Vfs, dir: &Path) {
    let file = dir.join("modes.txt");
    write_file(vfs, &file, WriteMode::CreateNew, b"first").await;
    assert_eq!(read_file(vfs, &file).await, b"first");
    let again = code_of(
        vfs.open_write(&file, WriteMode::CreateNew).await,
        "CreateNew on an existing file",
    );
    assert_eq!(again, "already_exists");

    write_file(vfs, &file, WriteMode::Append, b"+more").await;
    assert_eq!(read_file(vfs, &file).await, b"first+more", "Append");
    let mut rest = Vec::new();
    vfs.open_read_at(&file, 5)
        .await
        .unwrap_or_else(|error| panic!("open {} at an offset: {error}", file.display()))
        .read_to_end(&mut rest)
        .await
        .expect("read from an offset");
    assert_eq!(rest, b"+more", "open_read_at continues at the offset");
    write_file(vfs, &file, WriteMode::Truncate, b"new").await;
    assert_eq!(read_file(vfs, &file).await, b"new", "Truncate");
    write_file(vfs, &dir.join("appended.txt"), WriteMode::Append, b"a").await;
    assert_eq!(
        read_file(vfs, &dir.join("appended.txt")).await,
        b"a",
        "Append creates a missing file"
    );
}

/// A listing names files and directories, with kind and size, and no `.` or `..`.
pub async fn listing_describes_files_and_directories(vfs: &dyn Vfs, dir: &Path) {
    write_file(vfs, &dir.join("a.txt"), WriteMode::CreateNew, b"hello").await;
    vfs.mkdir(&dir.join("sub"), false).await.expect("mkdir sub");

    let entries = listing(vfs, dir).await;
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, ["a.txt", "sub"], "the listing of {}", dir.display());
    assert_eq!(entries[0].kind, EntryKind::File);
    assert_eq!(entries[0].size, 5);
    assert!(
        entries[0].modified.is_some(),
        "a file has a modification time"
    );
    assert_eq!(entries[1].kind, EntryKind::Dir);
    assert_eq!(entries[1].size, 0, "a directory has size 0");
    assert!(entries[1].is_dir_like());
    assert_eq!(
        batched(vfs, dir).await,
        entries,
        "list_batches lists what list does"
    );
}

/// The entries of `dir` from [`Vfs::list_batches`], however the backend cuts them, by name.
async fn batched(vfs: &dyn Vfs, dir: &Path) -> Vec<Entry> {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(4);
    let collect = async {
        let mut all = Vec::new();
        while let Some(batch) = receiver.recv().await {
            all.extend(batch);
        }
        all
    };
    let (listed, mut all) = tokio::join!(vfs.list_batches(dir, sender), collect);
    listed.unwrap_or_else(|error| panic!("list {} in batches: {error}", dir.display()));
    all.sort_by(|a, b| a.name.cmp(&b.name));
    all
}

/// `stat` describes the entry itself; a missing one is `not_found`.
pub async fn stat_describes_one_entry(vfs: &dyn Vfs, dir: &Path) {
    let file = dir.join("b.bin");
    write_file(vfs, &file, WriteMode::CreateNew, &[0; 1024]).await;
    let entry = vfs.stat(&file).await.expect("stat a file");
    assert_eq!(entry.name, "b.bin");
    assert_eq!(entry.kind, EntryKind::File);
    assert_eq!(entry.size, 1024);
    assert_eq!(
        vfs.stat(dir).await.expect("stat a directory").kind,
        EntryKind::Dir
    );
    let missing = code_of(vfs.stat(&dir.join("nope")).await, "stat a missing path");
    assert_eq!(missing, "not_found");
}

/// Reading a directory, listing a file and touching a missing path fail by kind.
pub async fn wrong_kinds_are_typed_errors(vfs: &dyn Vfs, dir: &Path) {
    let file = dir.join("d.txt");
    write_file(vfs, &file, WriteMode::CreateNew, b"x").await;
    let missing = dir.join("nope");

    let cases = [
        (
            code_of(vfs.open_read(dir).await, "read a directory"),
            "is_a_directory",
        ),
        (
            code_of(vfs.list(&file).await, "list a file"),
            "not_a_directory",
        ),
        (
            code_of(vfs.list(&missing).await, "list a missing path"),
            "not_found",
        ),
        (
            code_of(vfs.open_read(&missing).await, "read a missing path"),
            "not_found",
        ),
    ];
    for (index, (got, want)) in cases.into_iter().enumerate() {
        assert_eq!(got, want, "case {index}");
    }
}

/// `mkdir` creates parents only when recursive; recursive accepts an existing directory.
pub async fn mkdir_with_and_without_parents(vfs: &dyn Vfs, dir: &Path) {
    let deep = dir.join("x").join("y").join("z");
    let flat = code_of(vfs.mkdir(&deep, false).await, "mkdir without parents");
    assert_eq!(flat, "not_found");
    vfs.mkdir(&deep, true).await.expect("mkdir with parents");
    vfs.mkdir(&deep, true)
        .await
        .expect("a recursive mkdir accepts an existing directory");
    let twice = code_of(vfs.mkdir(&deep, false).await, "mkdir an existing directory");
    assert_eq!(twice, "already_exists");
    assert_eq!(vfs.stat(&deep).await.expect("stat").kind, EntryKind::Dir);
}

/// `rename` renames in place, moves into another directory where the backend can, and a
/// missing source is `not_found`.
pub async fn rename_within_the_backend(vfs: &dyn Vfs, dir: &Path) {
    let from = dir.join("e.txt");
    let renamed = dir.join("e1.txt");
    write_file(vfs, &from, WriteMode::CreateNew, b"e").await;
    vfs.rename(&from, &renamed).await.expect("rename in place");
    assert_eq!(read_file(vfs, &renamed).await, b"e");
    let gone = code_of(vfs.stat(&from).await, "stat the old name");
    assert_eq!(gone, "not_found");

    let mut last = renamed;
    if vfs.capabilities().rename_across_dirs {
        let target_dir = dir.join("moved");
        vfs.mkdir(&target_dir, false).await.expect("mkdir");
        let moved = target_dir.join("e2.txt");
        vfs.rename(&last, &moved)
            .await
            .expect("move into a directory");
        assert_eq!(read_file(vfs, &moved).await, b"e");
        last = moved;
    }
    let source_gone = code_of(
        vfs.rename(&dir.join("e.txt"), &last).await,
        "rename a missing source",
    );
    assert_eq!(source_gone, "not_found");
}

/// Files and empty directories go; a full directory only with `recursive`.
pub async fn remove_files_and_directories(vfs: &dyn Vfs, dir: &Path) {
    let sub = dir.join("full");
    vfs.mkdir(&sub, false).await.expect("mkdir");
    write_file(vfs, &sub.join("f.txt"), WriteMode::CreateNew, b"f").await;

    let not_empty = code_of(vfs.remove_dir(&sub, false).await, "remove a full directory");
    assert_eq!(not_empty, "directory_not_empty");
    vfs.remove_file(&sub.join("f.txt"))
        .await
        .expect("remove a file");
    vfs.remove_dir(&sub, false)
        .await
        .expect("remove an empty directory");

    vfs.mkdir(&sub.join("a").join("b"), true)
        .await
        .expect("mkdir");
    write_file(
        vfs,
        &sub.join("a").join("b").join("g.txt"),
        WriteMode::CreateNew,
        b"g",
    )
    .await;
    vfs.remove_dir(&sub, true).await.expect("remove a tree");
    assert!(listing(vfs, dir).await.is_empty(), "the tree is gone");

    let missing = code_of(vfs.remove_file(&sub).await, "remove a missing file");
    assert_eq!(missing, "not_found");
}

/// Where the backend sets times, `stat` reads back what was set (whole seconds, which every
/// backend keeps).
pub async fn set_mtime_is_what_stat_reads_back(vfs: &dyn Vfs, dir: &Path) {
    if !vfs.capabilities().set_mtime {
        return;
    }
    let file = dir.join("h.txt");
    write_file(vfs, &file, WriteMode::CreateNew, b"h").await;
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    vfs.set_mtime(&file, when).await.expect("set mtime");
    assert_eq!(vfs.stat(&file).await.expect("stat").modified, Some(when));
}

/// Where the backend reports free space, it is a number above zero.
pub async fn free_space_when_offered(vfs: &dyn Vfs, dir: &Path) {
    if !vfs.capabilities().free_space {
        return;
    }
    let free = vfs.free_space(dir).await.expect("free space");
    assert!(
        free.is_some_and(|bytes| bytes > 0),
        "free space was {free:?}"
    );
}

/// Names that differ only in case are two entries on a case-sensitive backend and one
/// otherwise.
pub async fn case_follows_the_capability(vfs: &dyn Vfs, dir: &Path) {
    write_file(vfs, &dir.join("case.txt"), WriteMode::CreateNew, b"lower").await;
    let upper = dir.join("CASE.TXT");
    if vfs.capabilities().case_sensitive {
        let missing = code_of(vfs.stat(&upper).await, "stat the other case");
        assert_eq!(missing, "not_found");
        write_file(vfs, &upper, WriteMode::CreateNew, b"upper").await;
        assert_eq!(listing(vfs, dir).await.len(), 2);
    } else {
        assert_eq!(read_file(vfs, &upper).await, b"lower");
        let taken = code_of(
            vfs.open_write(&upper, WriteMode::CreateNew).await,
            "CreateNew in the other case",
        );
        assert_eq!(taken, "already_exists");
    }
}

/// Streams a file from `from` into `to`, `open_read` straight into `open_write`, with no file
/// in between: afterwards each directory holds exactly its one file. `from_dir` and `to_dir`
/// must exist and be empty.
pub async fn copy_between(from: &dyn Vfs, from_dir: &Path, to: &dyn Vfs, to_dir: &Path) {
    // More than one buffer of `tokio::io::copy`, and not a multiple of one.
    let payload: Vec<u8> = (0..(1 << 20) + 17)
        .map(|index: u32| (index % 251) as u8)
        .collect();
    let source = from_dir.join("source.bin");
    let target = to_dir.join("copy.bin");
    write_file(from, &source, WriteMode::CreateNew, &payload).await;

    let mut reader = from.open_read(&source).await.expect("open the source");
    let mut writer = to
        .open_write(&target, WriteMode::CreateNew)
        .await
        .expect("open the target");
    let copied = tokio::io::copy(&mut reader, &mut writer)
        .await
        .expect("stream from one backend into the other");
    writer.shutdown().await.expect("shutdown the target");

    assert_eq!(copied, payload.len() as u64);
    assert_eq!(read_file(to, &target).await, payload, "the copy differs");
    let names = |entries: Vec<Entry>| -> Vec<String> {
        entries.into_iter().map(|entry| entry.name).collect()
    };
    assert_eq!(names(listing(from, from_dir).await), ["source.bin"]);
    assert_eq!(names(listing(to, to_dir).await), ["copy.bin"]);
}

/// Writes the tree [`run_read_only`] expects under `root`, which must exist and be empty:
/// `a.txt` (`hello`), `b.bin` (1024 zero bytes) and the empty directory `sub`. A read-only
/// backend brings the same tree in its fixture.
pub async fn prepare_read_only(vfs: &dyn Vfs, root: &Path) {
    write_file(vfs, &root.join("a.txt"), WriteMode::CreateNew, b"hello").await;
    write_file(vfs, &root.join("b.bin"), WriteMode::CreateNew, &[0; 1024]).await;
    vfs.mkdir(&root.join("sub"), false)
        .await
        .expect("mkdir sub");
}

/// Every check that only reads, against the tree of [`prepare_read_only`] under `root`.
pub async fn run_read_only(vfs: &dyn Vfs, root: &Path) {
    let entries = listing(vfs, root).await;
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        names,
        ["a.txt", "b.bin", "sub"],
        "the listing of {}",
        root.display()
    );
    assert_eq!(entries[0].kind, EntryKind::File);
    assert_eq!(entries[0].size, 5);
    assert!(
        entries[0].modified.is_some(),
        "a file has a modification time"
    );
    assert_eq!(entries[1].size, 1024);
    assert_eq!(entries[2].kind, EntryKind::Dir);
    assert_eq!(entries[2].size, 0, "a directory has size 0");
    assert!(entries[2].is_dir_like());

    let stat = vfs.stat(&root.join("b.bin")).await.expect("stat a file");
    assert_eq!((stat.name.as_str(), stat.size), ("b.bin", 1024));
    assert_eq!(stat.kind, EntryKind::File);
    let dir = vfs.stat(&root.join("sub")).await.expect("stat a directory");
    assert_eq!(dir.kind, EntryKind::Dir);

    assert_eq!(read_file(vfs, &root.join("a.txt")).await, b"hello");
    assert_eq!(read_file(vfs, &root.join("b.bin")).await, [0; 1024]);

    let missing = root.join("nope");
    let cases = [
        (
            code_of(vfs.stat(&missing).await, "stat a missing path"),
            "not_found",
        ),
        (
            code_of(vfs.open_read(&root.join("sub")).await, "read a directory"),
            "is_a_directory",
        ),
        (
            code_of(vfs.list(&root.join("a.txt")).await, "list a file"),
            "not_a_directory",
        ),
        (
            code_of(vfs.list(&missing).await, "list a missing path"),
            "not_found",
        ),
        (
            code_of(vfs.open_read(&missing).await, "read a missing path"),
            "not_found",
        ),
    ];
    for (index, (got, want)) in cases.into_iter().enumerate() {
        assert_eq!(got, want, "read-only case {index}");
    }
    seekable_read_when_offered(vfs, &root.join("a.txt")).await;
}

/// A seekable read starts where it was sought to; a backend that only streams says
/// `unsupported`.
pub async fn seekable_read_when_offered(vfs: &dyn Vfs, file: &Path) {
    match vfs.open_read_seekable(file).await {
        Err(error) => assert_eq!(error.code(), "unsupported", "seekable read: {error}"),
        Ok(mut stream) => {
            stream.seek(SeekFrom::Start(1)).await.expect("seek");
            let mut rest = Vec::new();
            stream
                .read_to_end(&mut rest)
                .await
                .expect("read after a seek");
            assert_eq!(rest, b"ello");
        }
    }
}

/// Every change fails with `unsupported` on a read-only backend, and the tree of
/// [`prepare_read_only`] under `root` is as it was.
pub async fn writes_are_unsupported(vfs: &dyn Vfs, root: &Path) {
    let file = root.join("a.txt");
    let new = root.join("new");
    let codes = [
        code_of(
            vfs.open_write(&new, WriteMode::CreateNew).await,
            "open_write",
        ),
        code_of(vfs.open_write(&file, WriteMode::Truncate).await, "truncate"),
        code_of(vfs.mkdir(&new, false).await, "mkdir"),
        code_of(vfs.rename(&file, &new).await, "rename"),
        code_of(vfs.remove_file(&file).await, "remove_file"),
        code_of(vfs.remove_dir(&root.join("sub"), true).await, "remove_dir"),
        code_of(
            vfs.set_mtime(&file, SystemTime::UNIX_EPOCH).await,
            "set_mtime",
        ),
        code_of(
            vfs.set_attributes(&file, crate::SetAttributes::default())
                .await,
            "set_attributes",
        ),
    ];
    assert!(
        codes.iter().all(|code| *code == "unsupported"),
        "codes of the writes: {codes:?}"
    );
    assert_eq!(read_file(vfs, &file).await, b"hello", "a.txt is unchanged");
    assert_eq!(listing(vfs, root).await.len(), 3, "nothing was added");
}
