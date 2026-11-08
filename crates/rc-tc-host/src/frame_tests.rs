use std::io::Cursor;

use super::*;

fn round_trip(frame: &Frame) -> Frame {
    let mut wire = Vec::new();
    write_frame(&mut wire, frame).expect("written");
    read_frame(&mut Cursor::new(wire)).expect("read back")
}

#[test]
fn control_and_data_frames_survive_the_pipe() {
    let control = Frame::Control(br#"{"jsonrpc":"2.0","id":1}"#.to_vec());
    assert_eq!(round_trip(&control), control);
    let data = Frame::Data {
        stream: 7,
        flags: END,
        bytes: vec![0, 255, 10, 13],
    };
    assert_eq!(round_trip(&data), data);
    let empty = Frame::Data {
        stream: u32::MAX,
        flags: 0,
        bytes: Vec::new(),
    };
    assert_eq!(round_trip(&empty), empty);
}

#[test]
fn a_clean_end_before_a_frame_is_closed() {
    assert!(matches!(
        read_frame(&mut Cursor::new(Vec::new())),
        Err(FrameError::Closed)
    ));
}

#[test]
fn an_end_inside_a_frame_is_an_error() {
    let mut wire = Vec::new();
    write_frame(&mut wire, &Frame::Control(b"{}".to_vec())).expect("written");
    wire.truncate(wire.len() - 1);
    assert!(matches!(
        read_frame(&mut Cursor::new(wire)),
        Err(FrameError::Io(_))
    ));
}

#[test]
fn lengths_outside_the_limit_are_refused() {
    for length in [0, MAX_FRAME + 1, u32::MAX] {
        let wire = length.to_le_bytes().to_vec();
        assert!(matches!(
            read_frame(&mut Cursor::new(wire)),
            Err(FrameError::Length(found)) if found == length
        ));
    }
}

#[test]
fn unknown_types_and_short_data_frames_are_refused() {
    let wire = [2_u32.to_le_bytes().as_slice(), &[9, 0]].concat();
    assert!(matches!(
        read_frame(&mut Cursor::new(wire)),
        Err(FrameError::Type(9))
    ));
    let wire = [3_u32.to_le_bytes().as_slice(), &[DATA, 1, 2]].concat();
    assert!(matches!(
        read_frame(&mut Cursor::new(wire)),
        Err(FrameError::ShortData)
    ));
}

#[test]
fn a_frame_over_the_limit_is_not_written() {
    let mut wire = Vec::new();
    let big = Frame::Control(vec![b' '; MAX_FRAME as usize]);
    assert!(write_frame(&mut wire, &big).is_err());
    assert!(wire.is_empty());
}
