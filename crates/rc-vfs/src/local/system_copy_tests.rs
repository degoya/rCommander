use std::{
    io,
    path::Path,
    sync::Arc,
    time::{Duration, SystemTime},
};

use super::copy;
use crate::CopyWatch;

/// More than two pieces of the Linux copy, so it goes round its loop.
const LEN: usize = 9 << 20;

fn payload(len: usize) -> Vec<u8> {
    (0..len).map(|at| (at % 251) as u8).collect()
}

fn source(dir: &Path, len: usize) -> std::path::PathBuf {
    let from = dir.join("from.bin");
    std::fs::write(&from, payload(len)).expect("write");
    from
}

#[test]
fn the_contents_arrive_whole_and_the_length_comes_back() {
    let dir = tempfile::tempdir().expect("tempdir");
    let from = source(dir.path(), LEN);
    let to = dir.path().join("to.bin");
    let watch = CopyWatch::new();
    let copied = copy(&from, &to, &watch).expect("copied");
    assert_eq!(copied, LEN as u64);
    assert_eq!(watch.copied(), LEN as u64, "the watch saw every byte");
    assert_eq!(std::fs::read(&to).expect("read"), payload(LEN));
}

#[test]
fn an_existing_target_is_refused_and_stays() {
    let dir = tempfile::tempdir().expect("tempdir");
    let from = source(dir.path(), 4096);
    let to = dir.path().join("to.bin");
    std::fs::write(&to, b"mine").expect("write");
    let refused = copy(&from, &to, &CopyWatch::new()).map_err(|error| error.kind());
    assert_eq!(refused, Err(io::ErrorKind::AlreadyExists));
    assert_eq!(std::fs::read(&to).expect("read"), b"mine");
}

#[test]
fn a_cancelled_copy_leaves_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let from = source(dir.path(), LEN);
    let to = dir.path().join("to.bin");
    let watch = CopyWatch::new();
    watch.cancel();
    let stopped = copy(&from, &to, &watch).map_err(|error| error.kind());
    assert_eq!(stopped, Err(io::ErrorKind::Interrupted));
    assert!(!to.exists(), "no half file");
}

#[test]
fn a_paused_copy_waits_and_finishes_when_let_go() {
    let dir = tempfile::tempdir().expect("tempdir");
    let from = source(dir.path(), LEN);
    let to = dir.path().join("to.bin");
    let watch = Arc::new(CopyWatch::new());
    watch.set_paused(true);
    let copier = {
        let (watch, from, to) = (Arc::clone(&watch), from.clone(), to.clone());
        std::thread::spawn(move || copy(&from, &to, &watch).map_err(|error| error.kind()))
    };
    std::thread::sleep(Duration::from_millis(200));
    assert!(!copier.is_finished(), "the copy waits while paused");
    assert_eq!(watch.copied(), 0, "nothing copied while paused");
    watch.set_paused(false);
    let copied = copier.join().expect("the copy ran");
    assert_eq!(copied, Ok(LEN as u64));
    assert_eq!(std::fs::read(&to).expect("read"), payload(LEN));
}

#[test]
fn the_copy_has_the_time_of_a_new_file_not_the_sources() {
    let dir = tempfile::tempdir().expect("tempdir");
    let from = source(dir.path(), 4096);
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    std::fs::File::options()
        .write(true)
        .open(&from)
        .expect("open")
        .set_modified(old)
        .expect("set the time");
    let to = dir.path().join("to.bin");
    copy(&from, &to, &CopyWatch::new()).expect("copied");
    let modified = std::fs::metadata(&to)
        .and_then(|meta| meta.modified())
        .expect("time");
    assert!(modified > old, "the caller sets the time a job keeps");
}

/// `CopyFileExW` carries the source's flags; the copy has a new file's, as a stream's has.
#[cfg(windows)]
#[test]
fn a_read_only_source_gives_a_writable_copy() {
    let dir = tempfile::tempdir().expect("tempdir");
    let from = source(dir.path(), 4096);
    let mut permissions = std::fs::metadata(&from).expect("stat").permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&from, permissions).expect("read-only");
    let to = dir.path().join("to.bin");
    copy(&from, &to, &CopyWatch::new()).expect("copied");
    let readonly = std::fs::metadata(&to)
        .expect("stat")
        .permissions()
        .readonly();
    assert!(!readonly, "the caller sets the flags a job keeps");
}

/// RC-110-56: the Mark-of-the-Web comes along with `CopyFileExW` itself, byte for byte.
#[cfg(windows)]
#[test]
fn the_zone_stream_comes_along() {
    let dir = tempfile::tempdir().expect("tempdir");
    let from = source(dir.path(), 4096);
    let zone = b"[ZoneTransfer]\r\nZoneId=3\r\n";
    let stream = |file: &Path| {
        let mut name = file.as_os_str().to_os_string();
        name.push(":Zone.Identifier");
        std::path::PathBuf::from(name)
    };
    std::fs::write(stream(&from), zone).expect("zone");
    let to = dir.path().join("to.bin");
    copy(&from, &to, &CopyWatch::new()).expect("copied");
    assert_eq!(std::fs::read(stream(&to)).expect("zone of the copy"), zone);
}
