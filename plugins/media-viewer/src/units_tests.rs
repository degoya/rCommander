use super::*;

#[test]
fn durations_read_as_clock_times() {
    assert_eq!(duration_millis(2_612), "0:02.612");
    assert_eq!(duration_millis(3_723_004), "1:02:03.004");
    assert_eq!(duration_seconds(12.345).as_deref(), Some("0:12.345"));
    assert_eq!(duration_seconds(f64::NAN), None);
    assert_eq!(duration_seconds(-1.0), None);
    assert_eq!(duration_seconds(f64::INFINITY), None);
    assert_eq!(
        duration_scaled(900_000, 90_000).as_deref(),
        Some("0:10.000")
    );
    assert_eq!(duration_scaled(5, 0), None);
    assert_eq!(duration_scaled(u64::MAX, 1), None);
}

#[test]
fn codes_read_as_names() {
    assert_eq!(channels(2), "2 (stereo)");
    assert_eq!(channels(3), "3");
    assert_eq!(fourcc(b"\xa9nam"), "\u{a9}nam");
    assert_eq!(fourcc(&[0x01, b'a', b'b', b'c']), "?abc");
    assert_eq!(bmff_codec(b"hvc1"), "H.265/HEVC (hvc1)");
    assert_eq!(bmff_codec(b"zzzz"), "zzzz");
    assert_eq!(matroska_codec("A_AAC/MPEG4/LC"), "AAC (A_AAC/MPEG4/LC)");
    assert_eq!(matroska_codec("V_UNKNOWN"), "V_UNKNOWN");
}
