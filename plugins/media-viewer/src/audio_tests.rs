use super::*;
use crate::fixtures::{PNG, flac, mp3};
use crate::fixtures_video::m4a;
use crate::test_support::{fact, named};

fn read_named(bytes: &[u8], name: &str, format: Audio, want_cover: bool) -> Media {
    read(&mut named(bytes, name), format, want_cover).expect("the file reads")
}

#[test]
fn an_mp3_names_its_duration_codec_and_tags() {
    let media = read_named(&mp3(), "song.mp3", Audio::Mpeg, true);
    let sections = &media.sections;
    assert_eq!(fact(sections, "Audio", "Format"), Some("MPEG audio (MP3)"));
    assert_eq!(fact(sections, "Audio", "Duration"), Some("0:02.612"));
    assert_eq!(fact(sections, "Audio", "Sample rate"), Some("44100 Hz"));
    assert_eq!(fact(sections, "Audio", "Channels"), Some("2 (stereo)"));
    assert_eq!(fact(sections, "Tags (ID3v2)", "Title"), Some("Song title"));
    assert_eq!(fact(sections, "Tags (ID3v2)", "Artist"), Some("The Artist"));
    assert_eq!(fact(sections, "Tags (ID3v2)", "Album"), Some("The Album"));
    assert_eq!(fact(sections, "Tags (ID3v2)", "Track"), Some("3"));
    assert_eq!(media.title.as_deref(), Some("Song title"));
    assert_eq!(media.artwork, None);
}

#[test]
fn a_flac_names_its_stream_tags_and_cover() {
    let media = read_named(&flac(), "song.flac", Audio::Flac, true);
    let sections = &media.sections;
    assert_eq!(fact(sections, "Audio", "Format"), Some("FLAC"));
    assert_eq!(fact(sections, "Audio", "Duration"), Some("0:10.000"));
    assert_eq!(fact(sections, "Audio", "Bit depth"), Some("16 bit"));
    assert_eq!(
        fact(sections, "Tags (Vorbis comments)", "Title"),
        Some("Flac title")
    );
    assert_eq!(
        fact(sections, "Tags (Vorbis comments)", "Artist"),
        Some("Flac artist")
    );
    let cover = fact(sections, "Pictures", "Front cover").expect("the picture is listed");
    assert!(cover.starts_with("png"), "{cover}");
    let artwork = media.artwork.expect("the cover is the artwork");
    assert_eq!(artwork.format, ImageFormat::Png);
    assert_eq!(artwork.bytes, PNG);
}

#[test]
fn a_flac_without_cover_reading_keeps_no_picture() {
    let media = read_named(&flac(), "song.flac", Audio::Flac, false);
    assert_eq!(media.artwork, None);
    assert_eq!(
        fact(&media.sections, "Tags (Vorbis comments)", "Title"),
        Some("Flac title")
    );
}

#[test]
fn an_m4a_names_its_duration_codec_and_tags() {
    let media = read_named(&m4a(), "song.m4a", Audio::Mp4, true);
    let sections = &media.sections;
    assert_eq!(fact(sections, "Audio", "Format"), Some("MP4 audio"));
    assert_eq!(fact(sections, "Audio", "Duration"), Some("0:05.000"));
    assert_eq!(fact(sections, "Audio", "Sample rate"), Some("44100 Hz"));
    assert_eq!(fact(sections, "Tags (MP4)", "Title"), Some("M4A title"));
    assert_eq!(fact(sections, "Tags (MP4)", "Artist"), Some("M4A artist"));
    assert_eq!(fact(sections, "Tags (MP4)", "Album"), Some("M4A album"));
    // The codec comes from our own box reader.
    let tracks = sections
        .iter()
        .find(|node| node.label == "Tracks")
        .expect("tracks");
    let track = tracks.find("Track 1").expect("track 1");
    assert_eq!(track.value.as_deref(), Some("Audio"));
    assert_eq!(
        track.find("Codec").and_then(|node| node.value.as_deref()),
        Some("AAC (mp4a)")
    );
}

#[test]
fn garbage_is_bad_audio() {
    let garbage = vec![0x42u8; 4096];
    let failure =
        read(&mut named(&garbage, "noise.mp3"), Audio::Mpeg, true).expect_err("garbage is no MP3");
    assert_eq!(failure.kind, FailureKind::BadData);
}

#[test]
fn a_cancel_ends_the_read() {
    let mut src = named(&flac(), "song.flac");
    src.stop_after = Some(0);
    let failure = read(&mut src, Audio::Flac, true).expect_err("cancelled");
    assert_eq!(failure.kind, FailureKind::Cancelled);
}

#[test]
fn keys_read_as_words() {
    assert_eq!(key_label(ItemKey::EncoderSoftware), "Encoder software");
    assert_eq!(split_words("CoverFront"), "Cover front");
}
