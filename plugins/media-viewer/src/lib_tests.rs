use super::*;
use crate::fixtures::{JPEG, PNG, flac, mp3, tiff};
use crate::fixtures_video::{mkv, mkv_info, mkv_tracks, mp4};
use crate::test_support::{VecSink, named};

fn open(bytes: &[u8], name: &str, cover: &str) -> (Viewer, Document) {
    Viewer::open(&mut named(bytes, name), Settings::from_values(Some(cover)))
        .expect("the document opens")
}

#[test]
fn a_cover_is_offered_as_a_picture_unchanged() {
    let (mut viewer, document) = open(&flac(), "song.flac", "on");
    assert_eq!(
        document.outputs,
        vec![Output::Tree, Output::Text, Output::Image]
    );
    assert_eq!(document.title.as_deref(), Some("Flac title"));
    let mut sink = VecSink::default();
    assert_eq!(viewer.image(&mut sink).expect("the picture"), "png");
    assert_eq!(sink.0, PNG);
}

#[test]
fn the_setting_cover_off_offers_no_picture() {
    let (mut viewer, document) = open(&flac(), "song.flac", "off");
    assert_eq!(document.outputs, vec![Output::Tree, Output::Text]);
    let failure = viewer
        .image(&mut VecSink::default())
        .expect_err("no picture");
    assert_eq!(failure.kind, FailureKind::NotSupported);
    let (_, document) = open(&mp4(true), "clip.mp4", "off");
    assert!(!document.outputs.contains(&Output::Image));
}

#[test]
fn an_mp4_cover_is_a_jpeg() {
    let (mut viewer, document) = open(&mp4(false), "clip.mp4", "on");
    assert_eq!(document.title.as_deref(), Some("Holiday"));
    let mut sink = VecSink::default();
    assert_eq!(viewer.image(&mut sink).expect("the picture"), "jpeg");
    assert_eq!(sink.0, JPEG);
}

#[test]
fn the_text_holds_the_tree_s_facts() {
    let (mut viewer, _) = open(&mp3(), "song.mp3", "on");
    let mut sink = VecSink::default();
    assert_eq!(viewer.text(&mut sink).expect("the text"), "utf-8");
    let text = String::from_utf8(sink.0).expect("UTF-8");
    assert!(text.contains("\n  Duration: 0:02.612\n"), "{text}");
    assert!(text.contains("\n  Title: Song title\n"), "{text}");
    assert_eq!(text, viewer.text_string());
    let sections = viewer.children(None).expect("sections");
    assert_eq!(sections[0].label, "Audio");
    let audio = viewer.children(Some(sections[0].id)).expect("facts");
    assert!(audio.iter().any(|node| node.label == "Duration"));
}

#[test]
fn a_photo_and_a_film_open_too() {
    let (viewer, document) = open(&tiff(false), "photo.nef", "on");
    assert_eq!(document.outputs, vec![Output::Tree, Output::Text]);
    assert!(viewer.text_string().contains("Make: Canon"));
    let (viewer, _) = open(&mkv(&[mkv_info(), mkv_tracks()]), "film.mkv", "on");
    assert!(viewer.text_string().contains("Resolution: 1280 x 720"));
}

#[test]
fn an_unknown_file_is_not_supported() {
    let result = Viewer::open(&mut named(b"%PDF-1.7", "doc.pdf"), Settings::default());
    let failure = result.err().expect("not opened");
    assert_eq!(failure.kind, FailureKind::NotSupported);
    assert_eq!(failure.code.as_deref(), Some(codes::NOT_MEDIA));
}

#[test]
fn probe_checks_the_signature_of_the_extension() {
    assert!(probe(&mut named(&mp3(), "song.mp3")));
    assert!(probe(&mut named(&flac(), "song.flac")));
    assert!(probe(&mut named(&mp4(true), "clip.mov")));
    assert!(probe(&mut named(&mkv(&[mkv_info()]), "film.webm")));
    assert!(probe(&mut named(&tiff(true), "raw.arw")));
    assert!(!probe(&mut named(b"just some text", "song.mp3")));
    assert!(!probe(&mut named(&flac(), "song.ogg")));
    assert!(!probe(&mut named(&mp4(true), "film.mkv")));
    assert!(!probe(&mut named(&mkv(&[mkv_info()]), "clip.mp4")));
    assert!(!probe(&mut named(b"", "photo.tif")));
    // Without a known extension, the bytes decide.
    assert!(probe(&mut named(&tiff(true), "photo")));
    assert!(!probe(&mut named(b"%PDF-1.7", "doc")));
}

#[test]
fn images_are_known_by_their_bytes() {
    assert_eq!(ImageFormat::sniff(PNG), Some(ImageFormat::Png));
    assert_eq!(ImageFormat::sniff(JPEG), Some(ImageFormat::Jpeg));
    assert_eq!(ImageFormat::sniff(b"GIF89a"), None);
}
