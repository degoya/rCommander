use super::*;
use crate::fixtures::JPEG;
use crate::fixtures_video::{bx, container, ftyp, mp4, mp4_moov, mvhd};
use crate::test_support::{Sparse, TestSource, fact, named};
use crate::types::FailureKind;
use crate::{ImageFormat, codes};

fn read_bytes(bytes: &[u8]) -> Result<Media, Failure> {
    read(&mut named(bytes, "clip.mp4"), true)
}

fn assert_bad(result: Result<Media, Failure>, code: &str) {
    let failure = result.expect_err("the file is refused");
    assert_eq!(failure.kind, FailureKind::BadData);
    assert_eq!(failure.code.as_deref(), Some(code));
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
fn moov_before_mdat_names_resolution_tracks_and_tags() {
    for moov_first in [true, false] {
        let media = read_bytes(&mp4(moov_first)).expect("the MP4 reads");
        assert_eq!(fact(&media.sections, "File", "Duration"), Some("0:10.000"));
        assert_eq!(
            fact(&media.sections, "File", "Container"),
            Some("ISO base media (brand isom)")
        );
        let video = track(&media, "Track 1");
        assert_eq!(video.value.as_deref(), Some("Video"));
        assert_eq!(value(video, "Codec"), Some("H.264/AVC (avc1)"));
        assert_eq!(value(video, "Resolution"), Some("1920 x 1080"));
        assert_eq!(value(video, "Duration"), Some("0:10.000"));
        assert_eq!(value(video, "Language"), Some("eng"));
        let audio = track(&media, "Track 2");
        assert_eq!(audio.value.as_deref(), Some("Audio"));
        assert_eq!(value(audio, "Codec"), Some("AAC (mp4a)"));
        assert_eq!(value(audio, "Channels"), Some("2 (stereo)"));
        assert_eq!(value(audio, "Sample rate"), Some("48000 Hz"));
        assert_eq!(fact(&media.sections, "Tags", "Title"), Some("Holiday"));
        assert_eq!(
            fact(&media.sections, "Tags", "Location"),
            Some("+48.1374+011.5755/")
        );
        assert_eq!(media.title.as_deref(), Some("Holiday"));
        let artwork = media.artwork.expect("the cover is the artwork");
        assert_eq!(artwork.format, ImageFormat::Jpeg);
        assert_eq!(artwork.bytes, JPEG);
    }
}

#[test]
fn a_cover_is_not_read_unless_wanted() {
    let media = read(&mut named(&mp4(true), "clip.mp4"), false).expect("the MP4 reads");
    assert_eq!(media.artwork, None);
    assert!(fact(&media.sections, "Tags", "Cover").is_some());
}

#[test]
fn moov_behind_gibibytes_of_media_is_found_reading_headers_only() {
    // An `mdat` of 6 GiB (a 64-bit size) between `ftyp` and `moov`, never held.
    let gap: u64 = 6 << 30;
    let mut head = ftyp(b"isom");
    head.extend_from_slice(&1u32.to_be_bytes());
    head.extend_from_slice(b"mdat");
    head.extend_from_slice(&(gap + 16).to_be_bytes());
    let mut src = TestSource::new(
        Sparse {
            head,
            gap,
            tail: mp4_moov(),
        },
        "clip.mp4",
    );
    let media = read(&mut src, true).expect("the MP4 reads");
    assert_eq!(fact(&media.sections, "File", "Duration"), Some("0:10.000"));
    assert!(src.bytes_read < 1 << 20, "read {} bytes", src.bytes_read);
}

#[test]
fn a_file_without_moov_is_named() {
    let bytes = [ftyp(b"isom"), bx(b"mdat", &[0; 64])].concat();
    assert_bad(read_bytes(&bytes), codes::NO_MOOV);
}

#[test]
fn a_box_larger_than_the_file_is_bad_data() {
    let mut bytes = ftyp(b"isom");
    bytes.extend_from_slice(&0x7FFF_FFF0u32.to_be_bytes());
    bytes.extend_from_slice(b"mdat");
    bytes.extend_from_slice(&[0; 64]);
    assert_bad(read_bytes(&bytes), codes::BAD_VIDEO);
}

#[test]
fn a_truncated_moov_is_bad_data() {
    let full = mp4(true);
    let moov_end = ftyp(b"isom").len() + mp4_moov().len();
    assert_bad(read_bytes(&full[..moov_end - 10]), codes::BAD_VIDEO);
}

#[test]
fn a_largesize_of_u64_max_is_bad_data_not_an_overflow() {
    let mut bytes = ftyp(b"isom");
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(b"mdat");
    bytes.extend_from_slice(&u64::MAX.to_be_bytes());
    bytes.extend_from_slice(&[0; 64]);
    assert_bad(read_bytes(&bytes), codes::BAD_VIDEO);
}

#[test]
fn a_size_below_its_header_is_bad_data() {
    let mut bytes = ftyp(b"isom");
    bytes.extend_from_slice(&4u32.to_be_bytes());
    bytes.extend_from_slice(b"free");
    bytes.extend_from_slice(&[0; 64]);
    assert_bad(read_bytes(&bytes), codes::BAD_VIDEO);
    let mut large = ftyp(b"isom");
    large.extend_from_slice(&1u32.to_be_bytes());
    large.extend_from_slice(b"free");
    large.extend_from_slice(&8u64.to_be_bytes());
    large.extend_from_slice(&[0; 64]);
    assert_bad(read_bytes(&large), codes::BAD_VIDEO);
}

#[test]
fn a_child_larger_than_its_parent_is_bad_data() {
    let mut trak = 0x1000u32.to_be_bytes().to_vec();
    trak.extend_from_slice(b"trak");
    trak.extend_from_slice(&[0; 32]);
    let moov = bx(b"moov", &[mvhd(1000, 1000), trak].concat());
    let bytes = [ftyp(b"isom"), moov, bx(b"mdat", &[0; 8192])].concat();
    assert_bad(read_bytes(&bytes), codes::BAD_VIDEO);
}

#[test]
fn a_moov_over_the_limit_is_too_large() {
    // The header claims more than 64 MiB; the body is never read.
    let claimed = MAX_MOOV + 9;
    let mut head = ftyp(b"isom");
    head.extend_from_slice(&u32::try_from(claimed).expect("fits").to_be_bytes());
    head.extend_from_slice(b"moov");
    let mut src = TestSource::new(
        Sparse {
            head,
            gap: claimed - 8,
            tail: Vec::new(),
        },
        "clip.mp4",
    );
    let failure = read(&mut src, true).expect_err("too large");
    assert_eq!(failure.code.as_deref(), Some(codes::TOO_LARGE));
}

#[test]
fn quicktime_languages_and_numbers_are_none() {
    assert_eq!(
        language_code(crate::fixtures_video::ENG).as_deref(),
        Some("eng")
    );
    assert_eq!(
        language_code(crate::fixtures_video::UND).as_deref(),
        Some("und")
    );
    assert_eq!(language_code(0), None);
    assert_eq!(language_code(0x7FFF), None);
}

#[test]
fn a_cancel_ends_the_walk() {
    // `moov` lies beyond the first window, so a second read is needed.
    let bytes = [ftyp(b"isom"), bx(b"mdat", &[0; 65_536]), mp4_moov()].concat();
    let mut src = named(&bytes, "clip.mp4");
    src.stop_after = Some(1);
    let failure = read(&mut src, true).expect_err("cancelled");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

#[test]
fn an_empty_container_list_is_no_moov() {
    let bytes = container(b"free", &[]);
    assert_bad(read_bytes(&bytes), codes::NO_MOOV);
}
