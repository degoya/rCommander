use std::{fs, path::Path, thread, time::Duration};

use tokio::sync::mpsc;

use super::{describe, send_in_parts};
use crate::{LocalFs, Vfs, VfsError};

/// An age no test reaches: the parts are cut by their size alone.
const NEVER: Duration = Duration::from_secs(3600);

/// A folder of `count` files.
fn folder(count: usize) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    for index in 0..count {
        fs::write(dir.path().join(format!("f{index:04}.txt")), b"x").expect("file");
    }
    dir
}

/// The names of `dir` in the order the plain listing reads them.
fn listed(dir: &Path) -> Vec<String> {
    let entries = describe::list(dir).expect("list");
    entries.into_iter().map(|entry| entry.name).collect()
}

#[test]
fn the_parts_arrive_in_order_and_complete() {
    let dir = folder(10);
    let (sender, mut receiver) = mpsc::channel(16);
    let read = send_in_parts(dir.path(), &sender, 3, NEVER).expect("listed");
    drop(sender);
    let mut parts = Vec::new();
    while let Some(part) = receiver.blocking_recv() {
        parts.push(part);
    }
    assert_eq!(read, 10);
    let sizes: Vec<usize> = parts.iter().map(Vec::len).collect();
    assert_eq!(sizes, [3, 3, 3, 1]);
    let names: Vec<String> = parts
        .into_iter()
        .flatten()
        .map(|entry| entry.name)
        .collect();
    assert_eq!(names, listed(dir.path()));
}

#[test]
fn a_part_older_than_its_age_goes_out_with_fewer_entries() {
    let dir = folder(5);
    let (sender, mut receiver) = mpsc::channel(16);
    send_in_parts(dir.path(), &sender, 4096, Duration::ZERO).expect("listed");
    drop(sender);
    let mut parts = Vec::new();
    while let Some(part) = receiver.blocking_recv() {
        parts.push(part.len());
    }
    assert_eq!(parts, [1, 1, 1, 1, 1]);
}

#[test]
fn an_empty_folder_sends_nothing() {
    let dir = folder(0);
    let (sender, mut receiver) = mpsc::channel(4);
    let read = send_in_parts(dir.path(), &sender, 3, NEVER).expect("listed");
    drop(sender);
    assert_eq!(read, 0);
    assert!(receiver.blocking_recv().is_none());
}

#[test]
fn a_receiver_that_is_gone_stops_the_reading() {
    let dir = folder(100);
    let (sender, mut receiver) = mpsc::channel(1);
    let path = dir.path().to_path_buf();
    let reader = thread::spawn(move || send_in_parts(&path, &sender, 1, NEVER));
    // The panel takes the first part and moves on to another folder.
    assert!(receiver.blocking_recv().is_some_and(|part| part.len() == 1));
    drop(receiver);
    let read = reader.join().expect("no panic").expect("no error");
    // At most the part taken, the one buffered and the one whose send found no receiver.
    assert!(
        read <= 3,
        "read {read} of 100 entries after the receiver was gone"
    );
}

#[test]
fn a_receiver_gone_before_the_start_reads_nothing() {
    let dir = folder(10);
    let (sender, receiver) = mpsc::channel(4);
    drop(receiver);
    let read = send_in_parts(dir.path(), &sender, 3, NEVER).expect("no error");
    assert_eq!(read, 0);
}

#[test]
fn a_missing_folder_is_not_found() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing = dir.path().join("missing");
    let (sender, _receiver) = mpsc::channel(4);
    let error = send_in_parts(&missing, &sender, 3, NEVER).expect_err("missing");
    assert!(matches!(error, VfsError::NotFound { .. }), "{error:?}");
}

#[tokio::test]
async fn the_local_backend_lists_in_parts_what_list_lists() {
    let dir = folder(10);
    let (sender, mut receiver) = mpsc::channel::<Vec<crate::Entry>>(4);
    let collect = async {
        let mut names = Vec::new();
        while let Some(part) = receiver.recv().await {
            names.extend(part.into_iter().map(|entry| entry.name));
        }
        names
    };
    let (listed_in_parts, mut names) =
        tokio::join!(LocalFs.list_batches(dir.path(), sender), collect);
    listed_in_parts.expect("listed");
    let mut all = listed(dir.path());
    names.sort();
    all.sort();
    assert_eq!(names, all);
}
