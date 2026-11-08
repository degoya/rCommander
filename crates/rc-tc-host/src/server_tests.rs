use std::io::Cursor;

use serde_json::json;

use super::*;
use crate::test_plugin::Fake;

/// Runs the loop over `requests` and returns what it ended with and every frame it wrote.
fn run(plugin: Fake, staging: &Path, requests: &[Frame]) -> (Result<(), ServeError>, Vec<Frame>) {
    let mut input = Vec::new();
    for frame in requests {
        write_frame(&mut input, frame).expect("request written");
    }
    let mut output = Vec::new();
    let ended =
        Server::new(plugin, staging.to_path_buf()).serve(&mut Cursor::new(input), &mut output);
    let mut frames = Vec::new();
    let mut output = Cursor::new(output);
    while let Ok(frame) = read_frame(&mut output) {
        frames.push(frame);
    }
    (ended, frames)
}

fn request(id: u64, name: &str, params: Value) -> Frame {
    Frame::Control(Message::request(id, name, params).to_json())
}

fn message(frame: &Frame) -> Message {
    let Frame::Control(json) = frame else {
        panic!("expected a control frame, got {frame:?}");
    };
    Message::from_json(json).expect("JSON-RPC")
}

fn absolute_path(name: &str) -> String {
    std::env::temp_dir()
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn hello_names_protocol_arch_and_kinds() {
    let staging = tempfile::tempdir().expect("staging");
    let (ended, frames) = run(
        Fake::with(Vec::new()),
        staging.path(),
        &[request(1, method::HELLO, Value::Null)],
    );
    ended.expect("clean end");
    let hello: Hello =
        serde_json::from_value(message(&frames[0]).result.expect("result")).expect("hello");
    assert_eq!(hello.protocol, PROTOCOL);
    assert_eq!(hello.kinds, vec![Kind::Wcx, Kind::Wlx]);
}

#[test]
fn an_entry_is_streamed_and_its_staged_copy_removed() {
    let staging = tempfile::tempdir().expect("staging");
    let params = json!({ "archive": absolute_path("a.zip"), "entry": "a.txt", "stream": 4 });
    let (ended, frames) = run(
        Fake::with(b"hello".to_vec()),
        staging.path(),
        &[request(2, method::WCX_EXTRACT, params)],
    );
    ended.expect("clean end");
    assert_eq!(
        frames[..2],
        [
            Frame::Data {
                stream: 4,
                flags: 0,
                bytes: b"hello".to_vec()
            },
            Frame::Data {
                stream: 4,
                flags: END,
                bytes: Vec::new()
            },
        ]
    );
    assert_eq!(message(&frames[2]).result, Some(json!({ "size": 5 })));
    assert_eq!(fs::read_dir(staging.path()).expect("listed").count(), 0);
}

#[test]
fn refusals_are_answers_and_the_loop_goes_on() {
    let staging = tempfile::tempdir().expect("staging");
    let (ended, frames) = run(
        Fake::with(Vec::new()),
        staging.path(),
        &[
            request(1, "wcx.format", Value::Null),
            request(2, method::WCX_LIST, json!({ "archive": "relative.zip" })),
            request(3, method::WCX_LIST, json!({ "path": 1 })),
            request(
                4,
                method::WCX_EXTRACT,
                json!({ "archive": absolute_path("a.zip"), "entry": "b", "stream": 1 }),
            ),
            request(5, method::HELLO, Value::Null),
        ],
    );
    ended.expect("clean end");
    let codes: Vec<_> = frames
        .iter()
        .map(|frame| message(frame).error.map(|error| error.code))
        .collect();
    assert_eq!(
        codes,
        [
            Some(code::METHOD_NOT_FOUND),
            Some(code::INVALID_PARAMS),
            Some(code::INVALID_PARAMS),
            Some(code::PLUGIN),
            None,
        ]
    );
    assert_eq!(fs::read_dir(staging.path()).expect("listed").count(), 0);
}

#[test]
fn a_preview_larger_than_asked_is_refused() {
    let staging = tempfile::tempdir().expect("staging");
    let mut plugin = Fake::with(Vec::new());
    plugin.oversized_preview = true;
    let params = json!({ "file": absolute_path("a.png"), "width": 8, "height": 8, "stream": 1 });
    let (ended, frames) = run(
        plugin,
        staging.path(),
        &[request(1, method::WLX_PREVIEW, params)],
    );
    ended.expect("clean end");
    assert_eq!(frames.len(), 1, "no pixels sent");
    assert_eq!(
        message(&frames[0]).error.map(|error| error.code),
        Some(code::PLUGIN)
    );
}

#[test]
fn data_frames_and_answers_to_the_host_end_the_loop() {
    let staging = tempfile::tempdir().expect("staging");
    let data = Frame::Data {
        stream: 1,
        flags: 0,
        bytes: vec![1],
    };
    let (ended, _) = run(Fake::with(Vec::new()), staging.path(), &[data]);
    assert!(matches!(ended, Err(ServeError::Protocol(_))));
    let answer = Frame::Control(Message::answer(1, Value::Null).to_json());
    let (ended, _) = run(Fake::with(Vec::new()), staging.path(), &[answer]);
    assert!(matches!(ended, Err(ServeError::Protocol(_))));
    let garbage = Frame::Control(b"{".to_vec());
    let (ended, _) = run(Fake::with(Vec::new()), staging.path(), &[garbage]);
    assert!(matches!(ended, Err(ServeError::Protocol(_))));
}

#[test]
fn a_late_credit_is_passed_over() {
    let staging = tempfile::tempdir().expect("staging");
    let credit = Frame::Control(
        Message::notification(method::CREDIT, json!({ "stream": 9, "bytes": 5 })).to_json(),
    );
    let (ended, frames) = run(
        Fake::with(Vec::new()),
        staging.path(),
        &[credit, request(1, method::HELLO, Value::Null)],
    );
    ended.expect("clean end");
    assert_eq!(frames.len(), 1);
}
