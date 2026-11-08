use super::*;

#[test]
fn extensions_name_their_reader() {
    assert_eq!(Kind::from_name("a.MP3"), Some(Kind::Audio(Audio::Mpeg)));
    assert_eq!(Kind::from_name("a.opus"), Some(Kind::Audio(Audio::Ogg)));
    assert_eq!(Kind::from_name("a.aif"), Some(Kind::Audio(Audio::Aiff)));
    assert_eq!(Kind::from_name("clip.mov"), Some(Kind::Bmff));
    assert_eq!(Kind::from_name("film.webm"), Some(Kind::Matroska));
    assert_eq!(Kind::from_name("raw.CR2"), Some(Kind::Tiff));
    assert_eq!(Kind::from_name("photo.heic"), Some(Kind::Heif));
    assert_eq!(Kind::from_name("notes.txt"), None);
    assert_eq!(Kind::from_name("README"), None);
}

#[test]
fn signatures_match_their_format() {
    assert!(
        Kind::Audio(Audio::Mpeg).matches(b"ID3\x04\0\0\0\0\0\0"),
        "Kind::Audio(Audio::Mpeg)"
    );
    assert!(
        Kind::Audio(Audio::Mpeg).matches(&[0xFF, 0xFB, 0x90, 0x00]),
        "Kind::Audio(Audio::Mpeg)"
    );
    assert!(
        Kind::Audio(Audio::Mpeg).matches(&[0, 0, 0xFF, 0xFB, 0x90, 0x00]),
        "Kind::Audio(Audio::Mpeg)"
    );
    assert!(
        Kind::Audio(Audio::Aac).matches(&[0xFF, 0xF1, 0x50, 0x80]),
        "Kind::Audio(Audio::Aac)"
    );
    assert!(
        Kind::Audio(Audio::Flac).matches(b"fLaC\0\0\0\x22"),
        "Kind::Audio(Audio::Flac)"
    );
    assert!(
        Kind::Audio(Audio::Ogg).matches(b"OggS\0\x02"),
        "Kind::Audio(Audio::Ogg)"
    );
    assert!(
        Kind::Audio(Audio::Mp4).matches(b"\0\0\0\x18ftypM4A "),
        "Kind::Audio(Audio::Mp4)"
    );
    assert!(
        Kind::Audio(Audio::Wav).matches(b"RIFF\0\0\0\0WAVEfmt "),
        "Kind::Audio(Audio::Wav)"
    );
    assert!(
        Kind::Audio(Audio::Aiff).matches(b"FORM\0\0\0\0AIFC"),
        "Kind::Audio(Audio::Aiff)"
    );
    assert!(
        Kind::Audio(Audio::Ape).matches(b"MAC \x96\x0f"),
        "Kind::Audio(Audio::Ape)"
    );
    assert!(
        Kind::Audio(Audio::WavPack).matches(b"wvpk\0\0\0\0"),
        "Kind::Audio(Audio::WavPack)"
    );
    assert!(Kind::Bmff.matches(b"\0\0\0\x18ftypisom"), "Kind::Bmff");
    assert!(
        Kind::Bmff.matches(b"\0\0\0\x08wide\0\0\0\0mdat"),
        "Kind::Bmff"
    );
    assert!(
        Kind::Matroska.matches(&[0x1A, 0x45, 0xDF, 0xA3, 0x9F]),
        "Kind::Matroska"
    );
    assert!(Kind::Tiff.matches(b"II*\0\x08\0\0\0"), "Kind::Tiff");
    assert!(Kind::Tiff.matches(b"MM\0*\0\0\0\x08"), "Kind::Tiff");
    assert!(Kind::Heif.matches(b"\0\0\0\x18ftypheic"), "Kind::Heif");
}

#[test]
fn other_signatures_do_not_match() {
    assert!(
        !Kind::Audio(Audio::Mpeg).matches(b"RIFF\0\0\0\0WAVE"),
        "Kind::Audio(Audio::Mpeg)"
    );
    assert!(
        !Kind::Audio(Audio::Mpeg).matches(&[0xFF, 0xF1, 0x50, 0x80]),
        "Kind::Audio(Audio::Mpeg)"
    );
    assert!(
        !Kind::Audio(Audio::Flac).matches(b"OggS"),
        "Kind::Audio(Audio::Flac)"
    );
    assert!(
        !Kind::Audio(Audio::Wav).matches(b"RIFF\0\0\0\0AVI "),
        "Kind::Audio(Audio::Wav)"
    );
    assert!(
        !Kind::Audio(Audio::Aiff).matches(b"FORM\0\0\0\08SVX"),
        "Kind::Audio(Audio::Aiff)"
    );
    assert!(!Kind::Bmff.matches(&[0x1A, 0x45, 0xDF, 0xA3]), "Kind::Bmff");
    assert!(
        !Kind::Matroska.matches(b"\0\0\0\x18ftypisom"),
        "Kind::Matroska"
    );
    assert!(!Kind::Tiff.matches(b"II+\0"), "Kind::Tiff");
    assert!(!Kind::Tiff.matches(b""), "Kind::Tiff");
    assert!(!Kind::Heif.matches(b"II*\0"), "Kind::Heif");
}

#[test]
fn a_nameless_file_is_sniffed() {
    assert_eq!(Kind::sniff(b"\0\0\0\x18ftypheic"), Some(Kind::Heif));
    assert_eq!(
        Kind::sniff(b"\0\0\0\x18ftypM4A "),
        Some(Kind::Audio(Audio::Mp4))
    );
    assert_eq!(Kind::sniff(b"\0\0\0\x18ftypmp42"), Some(Kind::Bmff));
    assert_eq!(Kind::sniff(&[0x1A, 0x45, 0xDF, 0xA3]), Some(Kind::Matroska));
    assert_eq!(Kind::sniff(b"fLaC"), Some(Kind::Audio(Audio::Flac)));
    assert_eq!(Kind::sniff(b"ID3\x04"), Some(Kind::Audio(Audio::Mpeg)));
    assert_eq!(Kind::sniff(b"%PDF-1.7"), None);
}
