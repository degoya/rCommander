use super::*;
use crate::fixtures_video::{ebml_header, el, el_uint, mkv, mkv_cluster, mkv_info, mkv_tracks};
use crate::test_support::{Sparse, TestSource, fact, named};
use crate::types::FailureKind;

fn read_bytes(bytes: &[u8]) -> Result<Media, Failure> {
    read(&mut named(bytes, "film.mkv"))
}

fn assert_bad(result: Result<Media, Failure>) {
    let failure = result.expect_err("the file is refused");
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(failure.code.as_deref(), Some(codes::BAD_VIDEO));
}

fn track<'a>(media: &'a Media, label: &str) -> &'a Node {
    media
        .sections
        .iter()
        .find(|node| node.label == "Tracks")
        .and_then(|tracks| tracks.find(label))
        .expect("the track is listed")
}

fn value<'a>(node: &'a Node, label: &str) -> Option<&'a str> {
    node.find(label).and_then(|child| child.value.as_deref())
}

#[test]
fn info_and_tracks_name_duration_resolution_and_codecs() {
    let media = read_bytes(&mkv(&[mkv_info(), mkv_tracks(), mkv_cluster()])).expect("reads");
    assert_eq!(fact(&media.sections, "File", "Container"), Some("Matroska"));
    assert_eq!(fact(&media.sections, "File", "Duration"), Some("0:12.345"));
    assert_eq!(fact(&media.sections, "File", "Title"), Some("Holiday"));
    assert_eq!(
        fact(&media.sections, "File", "Writing application"),
        Some("mkvmerge")
    );
    assert_eq!(media.title.as_deref(), Some("Holiday"));
    let video = track(&media, "Track 1");
    assert_eq!(video.value.as_deref(), Some("Video"));
    assert_eq!(value(video, "Codec"), Some("H.264/AVC (V_MPEG4/ISO/AVC)"));
    assert_eq!(value(video, "Resolution"), Some("1280 x 720"));
    assert_eq!(value(video, "Language"), Some("und"));
    let audio = track(&media, "Track 2");
    assert_eq!(audio.value.as_deref(), Some("Audio"));
    assert_eq!(value(audio, "Codec"), Some("Opus (A_OPUS)"));
    assert_eq!(value(audio, "Sample rate"), Some("48000 Hz"));
    assert_eq!(value(audio, "Channels"), Some("2 (stereo)"));
    assert_eq!(value(audio, "Language"), Some("de"));
}

#[test]
fn clusters_before_the_tracks_are_skipped_by_their_size() {
    let media = read_bytes(&mkv(&[
        mkv_cluster(),
        mkv_info(),
        mkv_cluster(),
        mkv_tracks(),
    ]))
    .expect("reads");
    assert_eq!(
        value(track(&media, "Track 1"), "Resolution"),
        Some("1280 x 720")
    );
}

#[test]
fn a_segment_of_unknown_size_runs_to_the_end_of_the_file() {
    let mut bytes = ebml_header("webm");
    bytes.extend_from_slice(&[
        0x18, 0x53, 0x80, 0x67, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    ]);
    bytes.extend(mkv_info());
    bytes.extend(mkv_tracks());
    let media = read_bytes(&bytes).expect("reads");
    assert_eq!(fact(&media.sections, "File", "Container"), Some("WebM"));
    assert_eq!(
        value(track(&media, "Track 2"), "Codec"),
        Some("Opus (A_OPUS)")
    );
}

#[test]
fn a_cluster_of_unknown_size_ends_the_walk() {
    let mut cluster = vec![0x1F, 0x43, 0xB6, 0x75, 0xFF];
    cluster.extend(el_uint(0xE7, 0));
    let media = read_bytes(&mkv(&[mkv_info(), cluster, mkv_tracks()])).expect("reads");
    assert_eq!(fact(&media.sections, "File", "Duration"), Some("0:12.345"));
    assert!(media.sections.iter().all(|node| node.label != "Tracks"));
}

#[test]
fn a_huge_file_is_walked_by_its_headers() {
    // A cluster of 4 GiB between `Info` and `Tracks`, never held.
    let gap: u64 = 4 << 30;
    let mut head = ebml_header("matroska");
    head.extend_from_slice(&[
        0x18, 0x53, 0x80, 0x67, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    ]);
    head.extend(mkv_info());
    head.extend_from_slice(&[0x1F, 0x43, 0xB6, 0x75, 0x01]);
    head.extend_from_slice(&gap.to_be_bytes()[1..]);
    let mut src = TestSource::new(
        Sparse {
            head,
            gap,
            tail: mkv_tracks(),
        },
        "film.mkv",
    );
    let media = read(&mut src).expect("reads");
    assert_eq!(
        value(track(&media, "Track 1"), "Resolution"),
        Some("1280 x 720")
    );
    assert!(src.bytes_read < 1 << 20, "read {} bytes", src.bytes_read);
}

#[test]
fn an_id_of_all_ones_is_bad_data() {
    let bad_child = [0xFF, 0x81, 0x00].to_vec();
    assert_bad(read_bytes(&mkv(&[mkv_info(), bad_child])));
}

#[test]
fn a_vint_of_all_ones_where_a_size_is_needed_is_bad_data() {
    // `Tracks` with the eight-byte unknown size: it cannot be read without its end.
    let mut tracks = vec![
        0x16, 0x54, 0xAE, 0x6B, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    ];
    tracks.extend(el_uint(0xAE, 0));
    assert_bad(read_bytes(&mkv(&[mkv_info(), tracks])));
    // A `TrackNumber` of unknown size inside a `TrackEntry`.
    let entry = el(0xAE, &[0xD7, 0xFF, 0x01]);
    assert_bad(read_bytes(&mkv(&[mkv_info(), el(0x1654_AE6B, &entry)])));
}

#[test]
fn a_size_beyond_the_file_or_a_first_byte_of_zero_is_bad_data() {
    let mut tracks = vec![
        0x16, 0x54, 0xAE, 0x6B, 0x01, 0x7F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE,
    ];
    tracks.extend(el_uint(0xAE, 0));
    let mut bytes = ebml_header("matroska");
    bytes.extend_from_slice(&[
        0x18, 0x53, 0x80, 0x67, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    ]);
    bytes.extend(tracks);
    assert_bad(read_bytes(&bytes));
    let zero = [0x16, 0x54, 0xAE, 0x6B, 0x00, 0x01].to_vec();
    assert_bad(read_bytes(&mkv(&[zero])));
}

#[test]
fn a_child_larger_than_its_parent_is_bad_data() {
    // A `TrackEntry` claiming 100 bytes inside a `Tracks` of 10.
    let mut body = vec![0xAE, 0xE4];
    body.extend_from_slice(&[0; 8]);
    assert_bad(read_bytes(&mkv(&[el(0x1654_AE6B, &body)])));
}

#[test]
fn a_file_that_is_no_matroska_is_bad_data() {
    assert_bad(read_bytes(b"not a matroska file at all"));
    assert_bad(read_bytes(&ebml_header("matroska")));
}
