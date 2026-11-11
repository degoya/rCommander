//! The streamed upload against the fake device (RC-110-16): a write returns only as the device
//! takes the bytes, so a copy's progress is the upload's and nothing is kept on this machine; a
//! stream dropped, cut short or written past its size leaves nothing on the device; and an open
//! download of the same device makes the write go through a temporary file as before.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use super::DEPTH;
use crate::{
    Vfs, WriteMode, WriteStream,
    device::{Abilities, DeviceFs, fake::FakeDevice},
};

/// The bytes of one write, as large as the fake device reads at a time.
const BLOCK: usize = 16 * 1024;
/// Long enough for a write that can go on to have gone on.
const SETTLE: Duration = Duration::from_millis(150);

fn device() -> (Arc<FakeDevice>, DeviceFs) {
    let device = FakeDevice::new(&["Internal storage"], Abilities::ALL);
    let fs = DeviceFs::new(Arc::<FakeDevice>::clone(&device));
    (device, fs)
}

fn file(name: &str) -> PathBuf {
    Path::new("/Internal storage").join(name)
}

fn payload(blocks: usize) -> Vec<u8> {
    (0..blocks * BLOCK)
        .map(|index| (index % 251) as u8)
        .collect()
}

async fn sized(fs: &DeviceFs, name: &str, size: usize) -> WriteStream {
    fs.open_write_sized(&file(name), WriteMode::CreateNew, size as u64)
        .await
        .expect("open sized")
}

/// Waits until `count` uploads of `device` have ended, kept or not.
async fn ended(device: &FakeDevice, count: usize) {
    for _ in 0..1000 {
        if device.uploads_ended.load(Ordering::SeqCst) >= count {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the upload did not end");
}

async fn read_back(fs: &DeviceFs, name: &str) -> Vec<u8> {
    let mut reader = fs.open_read(&file(name)).await.expect("open read");
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.expect("read");
    bytes
}

#[tokio::test]
async fn a_known_size_streams_as_the_device_takes_it_without_a_temporary_file() {
    let (device, fs) = device();
    let data = payload(12);
    device.hold.store(true, Ordering::SeqCst);
    let mut writer = sized(&fs, "big.bin", data.len()).await;
    let mut blocks = data.chunks(BLOCK);
    for _ in 0..DEPTH {
        let block = blocks.next().expect("block");
        tokio::time::timeout(SETTLE, writer.write_all(block))
            .await
            .expect("a write within the channel's depth returns")
            .expect("write");
    }
    // The device has taken nothing: one more write waits for it, where a temporary file would
    // have taken every block at once.
    let next = blocks.next().expect("block");
    assert!(
        tokio::time::timeout(SETTLE, writer.write_all(next))
            .await
            .is_err(),
        "a write runs no further ahead of the device than the channel's depth"
    );
    assert_eq!(device.uploaded.load(Ordering::SeqCst), 0);

    device.hold.store(false, Ordering::SeqCst);
    writer.write_all(next).await.expect("write");
    let mut written = ((DEPTH + 1) * BLOCK) as u64;
    for block in blocks {
        writer.write_all(block).await.expect("write");
        written += block.len() as u64;
        let taken = device.uploaded.load(Ordering::SeqCst);
        assert!(
            written - taken <= ((DEPTH + 2) * BLOCK) as u64,
            "written {written}, the device has {taken}: the progress is the upload's"
        );
    }
    writer.shutdown().await.expect("shutdown");
    assert_eq!(device.uploaded.load(Ordering::SeqCst), data.len() as u64);
    assert_eq!(read_back(&fs, "big.bin").await, data);
}

#[tokio::test]
async fn a_stream_dropped_during_the_upload_leaves_nothing_on_the_device() {
    let (device, fs) = device();
    let before = device.count();
    let data = payload(4 * DEPTH);
    let mut writer = sized(&fs, "dropped.bin", data.len()).await;
    // More than the channel holds, so the device is taking the file when the stream goes.
    for block in data[..2 * DEPTH * BLOCK].chunks(BLOCK) {
        writer.write_all(block).await.expect("write");
    }
    drop(writer);
    ended(&device, 1).await;
    assert_eq!(device.count(), before, "nothing was kept");
    assert_eq!(
        fs.stat(&file("dropped.bin"))
            .await
            .map(|_| ())
            .map_err(|e| e.code()),
        Err("not_found")
    );
}

#[tokio::test]
async fn a_stream_shut_down_short_of_its_size_fails_and_keeps_nothing() {
    let (device, fs) = device();
    let before = device.count();
    let data = payload(4);
    let mut writer = sized(&fs, "short.bin", data.len()).await;
    writer.write_all(&data[..BLOCK]).await.expect("write");
    assert!(writer.shutdown().await.is_err(), "short of the size");
    assert_eq!(device.uploads_ended.load(Ordering::SeqCst), 1);
    assert_eq!(device.count(), before, "nothing was kept");
}

#[tokio::test]
async fn a_write_past_the_size_fails_and_keeps_nothing() {
    let (device, fs) = device();
    let before = device.count();
    let mut writer = sized(&fs, "long.bin", 10).await;
    assert!(
        writer.write_all(&[7_u8; 11]).await.is_err(),
        "past the size"
    );
    assert!(writer.shutdown().await.is_err(), "the file is not complete");
    ended(&device, 1).await;
    assert_eq!(device.count(), before, "nothing was kept");
}

#[tokio::test]
async fn an_empty_file_of_size_zero_is_kept() {
    let (device, fs) = device();
    let mut writer = sized(&fs, "empty.txt", 0).await;
    writer.shutdown().await.expect("shutdown");
    assert_eq!(fs.stat(&file("empty.txt")).await.expect("stat").size, 0);
    assert_eq!(device.uploads_ended.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_open_download_of_the_device_makes_the_write_spool() {
    let (device, fs) = device();
    let mut first = fs
        .open_write(&file("source.bin"), WriteMode::CreateNew)
        .await
        .expect("open");
    first.write_all(b"from the device").await.expect("write");
    first.shutdown().await.expect("shutdown");

    // A copy within the device: its source is read while its target is written.
    let reader = fs.open_read(&file("source.bin")).await.expect("open read");
    device.hold.store(true, Ordering::SeqCst);
    let data = payload(DEPTH + 4);
    let mut writer = sized(&fs, "copy.bin", data.len()).await;
    tokio::time::timeout(SETTLE * 4, writer.write_all(&data))
        .await
        .expect("spooled, every write returns with the device held")
        .expect("write");
    drop(reader);
    device.hold.store(false, Ordering::SeqCst);
    writer.shutdown().await.expect("shutdown");
    assert_eq!(read_back(&fs, "copy.bin").await, data);
}
