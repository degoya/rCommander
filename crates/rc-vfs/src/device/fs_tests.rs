//! [`DeviceFs`] against the fake device: the conformance suite as far as MTP can pass it, the
//! path cache, what is refused before the device is asked, and a device that goes away.
//!
//! Not run from the suite: `write_modes` - MTP neither appends to a file nor reads one from an
//! offset, both refused as `unsupported` (checked below); `set_mtime` and `rename` across
//! folders skip themselves by the capabilities.

use std::{
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use tokio::io::AsyncWriteExt;

use super::DeviceFs;
use crate::{
    MemoryFs, Timeouts, Vfs, VfsError, WriteMode, conformance,
    device::{
        Abilities,
        fake::{FREE, FakeDevice},
    },
};

const STORAGE: &str = "Internal storage";

fn fs_over(device: &Arc<FakeDevice>) -> DeviceFs {
    DeviceFs::new(Arc::<FakeDevice>::clone(device))
}

fn storage() -> &'static Path {
    Path::new("/Internal storage")
}

fn code<T>(result: crate::Result<T>) -> &'static str {
    match result {
        Ok(_) => panic!("succeeded, an error was expected"),
        Err(error) => error.code(),
    }
}

#[tokio::test]
async fn the_device_passes_the_conformance_suite_as_far_as_mtp_can() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let fs = fs_over(&device);
    let root = storage().join("conformance");
    fs.mkdir(&root, false).await.expect("mkdir");
    let dir = |name: &str| root.join(name);
    for name in [
        "listing", "stat", "typed", "mkdir", "rename", "remove", "mtime", "space", "case",
    ] {
        fs.mkdir(&dir(name), false).await.expect("mkdir");
    }
    conformance::listing_describes_files_and_directories(&fs, &dir("listing")).await;
    conformance::stat_describes_one_entry(&fs, &dir("stat")).await;
    conformance::wrong_kinds_are_typed_errors(&fs, &dir("typed")).await;
    conformance::mkdir_with_and_without_parents(&fs, &dir("mkdir")).await;
    conformance::rename_within_the_backend(&fs, &dir("rename")).await;
    conformance::remove_files_and_directories(&fs, &dir("remove")).await;
    conformance::set_mtime_is_what_stat_reads_back(&fs, &dir("mtime")).await;
    conformance::free_space_when_offered(&fs, &dir("space")).await;
    conformance::case_follows_the_capability(&fs, &dir("case")).await;
}

#[tokio::test]
async fn the_read_only_part_of_the_suite_passes_too() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let fs = fs_over(&device);
    let root = storage().join("ro");
    fs.mkdir(&root, false).await.expect("mkdir");
    conformance::prepare_read_only(&fs, &root).await;
    conformance::run_read_only(&fs, &root).await;
}

#[tokio::test]
async fn files_stream_onto_the_device_and_off_it() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let fs = fs_over(&device);
    let memory = MemoryFs::new();
    for dir in ["/up", "/down"] {
        memory.mkdir(Path::new(dir), false).await.expect("mkdir");
    }
    for dir in ["up", "down"] {
        fs.mkdir(&storage().join(dir), false).await.expect("mkdir");
    }
    conformance::copy_between(&memory, Path::new("/up"), &fs, &storage().join("up")).await;
    conformance::copy_between(&fs, &storage().join("down"), &memory, Path::new("/down")).await;
}

#[tokio::test]
async fn the_root_lists_the_storages_as_folders() {
    let device = FakeDevice::new(&[STORAGE, "SD card"], Abilities::ALL);
    let fs = fs_over(&device);
    let mut names: Vec<String> = fs
        .list(Path::new("/"))
        .await
        .expect("list the root")
        .into_iter()
        .map(|entry| {
            assert!(entry.is_dir_like(), "a storage is a folder");
            entry.name
        })
        .collect();
    names.sort();
    assert_eq!(names, ["Internal storage", "SD card"]);
    assert!(fs.stat(Path::new("/")).await.expect("stat").is_dir_like());
    assert_eq!(
        fs.free_space(storage()).await.expect("free space"),
        Some(FREE)
    );
    assert_eq!(fs.free_space(Path::new("/")).await.expect("root"), None);
}

#[tokio::test]
async fn a_path_once_found_is_not_walked_again() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let fs = fs_over(&device);
    let deep = storage().join("a").join("b");
    fs.mkdir(&deep, true).await.expect("mkdir");
    let file = deep.join("f.txt");
    conformance::write_file(&fs, &file, WriteMode::CreateNew, b"x").await;
    let before = device.listings.load(Ordering::SeqCst);
    for _ in 0..3 {
        fs.stat(&file).await.expect("stat");
    }
    assert_eq!(
        device.listings.load(Ordering::SeqCst),
        before,
        "from the cache"
    );
}

#[tokio::test]
async fn copying_many_files_into_one_folder_lists_it_once() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let fs = fs_over(&device);
    let dir = storage().join("DCIM");
    fs.mkdir(&dir, false).await.expect("mkdir");
    fs.list(&dir).await.expect("list");
    let before = device.listings.load(Ordering::SeqCst);
    for index in 0..20 {
        let file = dir.join(format!("{index}.jpg"));
        let free = fs.stat(&file).await;
        assert_eq!(code(free), "not_found");
        conformance::write_file(&fs, &file, WriteMode::CreateNew, b"jpeg").await;
    }
    assert_eq!(
        device.listings.load(Ordering::SeqCst),
        before,
        "a complete folder answers for the names it does not hold"
    );
}

#[tokio::test]
async fn what_mtp_cannot_do_is_refused() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let fs = fs_over(&device);
    let file = storage().join("x.txt");
    conformance::write_file(&fs, &file, WriteMode::CreateNew, b"x").await;
    let codes = [
        code(fs.open_write(&file, WriteMode::Append).await),
        code(fs.open_read_at(&file, 1).await),
        code(fs.set_mtime(&file, std::time::SystemTime::UNIX_EPOCH).await),
        code(fs.mkdir(Path::new("/New storage"), false).await),
        code(fs.remove_dir(storage(), true).await),
        code(fs.rename(storage(), Path::new("/Other")).await),
    ];
    assert!(codes.iter().all(|code| *code == "unsupported"), "{codes:?}");
    let in_root = code(
        fs.open_write(Path::new("/loose.txt"), WriteMode::CreateNew)
            .await,
    );
    assert_eq!(in_root, "invalid_path", "a file goes into a storage");
    let moved = code(fs.rename(&file, &storage().join("sub").join("x.txt")).await);
    assert_eq!(moved, "unsupported", "no moves between folders");
}

#[tokio::test]
async fn what_the_device_does_not_do_is_refused_before_it_is_asked() {
    let none = Abilities {
        write: false,
        mkdir: false,
        delete: false,
        rename: false,
    };
    let device = FakeDevice::new(&[STORAGE], none);
    let fs = fs_over(&device);
    let objects = device.count();
    let listings = device.listings.load(Ordering::SeqCst);
    let file = storage().join("x");
    let codes = [
        code(fs.open_write(&file, WriteMode::CreateNew).await),
        code(fs.mkdir(&file, false).await),
        code(fs.remove_file(&file).await),
        code(fs.remove_dir(&file, true).await),
        code(fs.rename(&file, &storage().join("y")).await),
    ];
    assert!(codes.iter().all(|code| *code == "unsupported"), "{codes:?}");
    assert_eq!(device.count(), objects);
    assert_eq!(
        device.listings.load(Ordering::SeqCst),
        listings,
        "the device was not even asked"
    );
}

#[tokio::test]
async fn a_file_written_but_never_closed_leaves_nothing_on_the_device() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let fs = fs_over(&device);
    let objects = device.count();
    let mut stream = fs
        .open_write(&storage().join("half.bin"), WriteMode::CreateNew)
        .await
        .expect("open");
    stream.write_all(&[7; 100_000]).await.expect("write");
    drop(stream);
    assert_eq!(device.count(), objects, "nothing uploaded");
}

#[tokio::test]
async fn truncate_replaces_the_file_once_the_new_one_is_complete() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let fs = fs_over(&device);
    let file = storage().join("note.txt");
    conformance::write_file(&fs, &file, WriteMode::CreateNew, b"old").await;
    conformance::write_file(&fs, &file, WriteMode::Truncate, b"brand new").await;
    assert_eq!(conformance::read_file(&fs, &file).await, b"brand new");
    let names: Vec<String> = fs
        .list(storage())
        .await
        .expect("list")
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, ["note.txt"], "the old one is gone, not beside it");
}

#[tokio::test]
async fn an_unplugged_device_is_unavailable_not_empty() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let fs = fs_over(&device);
    device.gone.store(true, Ordering::SeqCst);
    assert_eq!(code(fs.list(Path::new("/")).await), "device_unavailable");
    assert_eq!(code(fs.stat(storage()).await), "device_unavailable");
}

#[tokio::test]
async fn a_device_that_hangs_fails_in_time() {
    let device = FakeDevice::new(&[STORAGE], Abilities::ALL);
    let limits = Timeouts {
        request: Duration::from_millis(50),
        listing: Duration::from_millis(50),
    };
    let fs = DeviceFs::with_limits(Arc::<FakeDevice>::clone(&device), 16, limits);
    device.stall.store(1_000, Ordering::SeqCst);
    let started = std::time::Instant::now();
    let answer = fs.list(Path::new("/")).await;
    assert!(
        matches!(answer, Err(VfsError::TimedOut { .. })),
        "{answer:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the caller is not held"
    );
}
