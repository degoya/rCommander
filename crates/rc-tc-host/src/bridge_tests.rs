use std::io::{PipeReader, PipeWriter, pipe};
use std::path::PathBuf;
use std::thread::{self, JoinHandle};

use super::*;
use crate::credit::WINDOW;
use crate::rpc::{Kind, code};
use crate::server::Server;
use crate::test_plugin::{Fake, entry};

type Ended = JoinHandle<Result<(), crate::server::ServeError>>;

/// The bridge over pipes to the loop on a thread of its own, as the host process would run it.
fn connect(plugin: Fake, staging: PathBuf) -> (Bridge<PipeReader, PipeWriter>, Ended) {
    let (from_host, to_bridge) = pipe().expect("pipe");
    let (from_bridge, to_host) = pipe().expect("pipe");
    let host = thread::spawn(move || {
        let (mut reader, mut writer) = (from_bridge, to_bridge);
        Server::new(plugin, staging).serve(&mut reader, &mut writer)
    });
    (Bridge::new(from_host, to_host), host)
}

/// A host played by hand: writes `frames` after the first request, then closes.
fn scripted(frames: Vec<Frame>) -> (Bridge<PipeReader, PipeWriter>, JoinHandle<()>) {
    let (from_host, mut to_bridge) = pipe().expect("pipe");
    let (mut from_bridge, to_host) = pipe().expect("pipe");
    let host = thread::spawn(move || {
        let _ = read_frame(&mut from_bridge);
        for frame in &frames {
            if write_frame(&mut to_bridge, frame).is_err() {
                break;
            }
        }
        // Keep reading credit until the bridge closes, so no write of the bridge blocks.
        while read_frame(&mut from_bridge).is_ok() {}
    });
    (Bridge::new(from_host, to_host), host)
}

fn archive() -> PathBuf {
    std::env::temp_dir().join("archive.wcx-test")
}

#[test]
fn hello_and_a_listing_without_the_unsafe_names() {
    let staging = tempfile::tempdir().expect("staging");
    let (mut bridge, host) = connect(Fake::with(b"abc".to_vec()), staging.path().to_path_buf());
    let hello = bridge.hello().expect("hello");
    assert_eq!(hello.kinds, vec![Kind::Wcx, Kind::Wlx]);
    let listing = bridge.list(&archive()).expect("listing");
    assert_eq!(listing.entries, vec![entry("a.txt", 3)]);
    assert_eq!(listing.rejected, 1);
    drop(bridge);
    host.join().expect("no panic").expect("clean end");
}

#[test]
fn a_large_entry_flows_under_the_credit_window() {
    let staging = tempfile::tempdir().expect("staging");
    let content: Vec<u8> = (0..3 * WINDOW + 17).map(|i| (i % 251) as u8).collect();
    let (mut bridge, host) = connect(Fake::with(content.clone()), staging.path().to_path_buf());
    let mut received = Vec::new();
    let size = bridge
        .extract(&archive(), "a.txt", &mut received)
        .expect("extracted");
    assert_eq!(size, content.len() as u64);
    assert!(received == content, "the bytes arrive unchanged");
    let error = bridge
        .extract(&archive(), "missing", &mut Vec::new())
        .expect_err("refused");
    assert!(matches!(
        error,
        BridgeError::Remote {
            code: code::PLUGIN,
            ..
        }
    ));
    assert!(!bridge.is_spent(), "a refusal leaves the host usable");
    drop(bridge);
    host.join().expect("no panic").expect("clean end");
}

#[test]
fn a_preview_arrives_as_bgra_rows() {
    let staging = tempfile::tempdir().expect("staging");
    let (mut bridge, _host) = connect(Fake::with(Vec::new()), staging.path().to_path_buf());
    let bitmap = bridge.preview(&archive(), 64, 64).expect("preview");
    assert_eq!((bitmap.width, bitmap.height, bitmap.bgra.len()), (2, 2, 16));
}

#[test]
fn a_crash_in_the_plugin_ends_only_the_host() {
    let staging = tempfile::tempdir().expect("staging");
    let mut plugin = Fake::with(Vec::new());
    plugin.crash = true;
    let (mut bridge, host) = connect(plugin, staging.path().to_path_buf());
    assert!(matches!(
        bridge.list(&archive()),
        Err(BridgeError::HostGone)
    ));
    assert!(host.join().is_err(), "the host's thread ended in the crash");
    assert!(bridge.is_spent());
    assert!(matches!(bridge.hello(), Err(BridgeError::HostGone)));
}

#[test]
fn data_beyond_the_window_breaks_the_protocol() {
    // Every frame before it was returned as credit at once, so one frame over the window is the
    // first the host could not have had credit for.
    let flood = Frame::Data {
        stream: 1,
        flags: 0,
        bytes: vec![0; WINDOW as usize + 1],
    };
    let (mut bridge, _host) = scripted(vec![flood]);
    let error = bridge
        .extract(&archive(), "a.txt", &mut Vec::new())
        .expect_err("violation");
    assert!(matches!(error, BridgeError::Protocol(_)), "{error}");
    assert!(bridge.is_spent());
}

#[test]
fn foreign_streams_wrong_ids_and_wrong_sizes_break_the_protocol() {
    let foreign = Frame::Data {
        stream: 99,
        flags: 0,
        bytes: vec![1],
    };
    let (mut bridge, _host) = scripted(vec![foreign]);
    let error = bridge
        .extract(&archive(), "a.txt", &mut Vec::new())
        .expect_err("violation");
    assert!(matches!(error, BridgeError::Protocol(_)), "{error}");

    let wrong_id = Frame::Control(Message::answer(42, json!({})).to_json());
    let (mut bridge, _host) = scripted(vec![wrong_id]);
    assert!(matches!(bridge.hello(), Err(BridgeError::Protocol(_))));

    let short = vec![
        Frame::Data {
            stream: 1,
            flags: 0,
            bytes: vec![1, 2],
        },
        Frame::Control(Message::answer(1, json!({ "size": 3 })).to_json()),
    ];
    let (mut bridge, _host) = scripted(short);
    let error = bridge
        .extract(&archive(), "a.txt", &mut Vec::new())
        .expect_err("violation");
    assert!(matches!(error, BridgeError::Protocol(_)), "{error}");
}

#[test]
fn an_oversized_preview_is_refused() {
    let pixels = vec![
        Frame::Data {
            stream: 1,
            flags: 0,
            bytes: vec![0; 2 * 2 * 4],
        },
        Frame::Control(Message::answer(1, json!({ "width": 2, "height": 2 })).to_json()),
    ];
    let (mut bridge, _host) = scripted(pixels);
    let error = bridge
        .preview(&archive(), 1, 1)
        .expect_err("bigger than asked");
    assert!(
        matches!(error, BridgeError::Sink(_) | BridgeError::Protocol(_)),
        "{error}"
    );
}
