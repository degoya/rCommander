//! Audio through lofty (0.25): duration, bitrate, sample rate, channels, bit depth, the format,
//! every tag with its common fields first, and the pictures, the front cover as artwork.
//!
//! lofty reads through [`SourceReader`], which fetches at most [`READ_BUDGET`] bytes in all, and
//! allocates at most [`ALLOCATION_LIMIT`] bytes for one tag item (its global option, set for
//! this thread before each read). Together they bound what lofty can hold - it keeps nothing it
//! has not read - well inside the store's 64 MiB. An MP4 audio file (`m4a`) also goes through our
//! own box reader for its tracks' codecs, which lofty's generic properties do not name.

use std::error::Error as _;

use lofty::config::{GlobalOptions, ParseOptions, ParsingMode, apply_global_options};
use lofty::error::{AllocationError, FileParseError};
use lofty::file::{AudioFile, FileType, TaggedFile, TaggedFileExt};
use lofty::picture::{Picture, PictureType};
use lofty::probe::Probe;
use lofty::tag::{ItemKey, ItemValue, Tag, TagType};

use crate::io::SourceReader;
use crate::kind::Audio;
use crate::outline::Node;
use crate::types::{Failure, FailureKind, Source};
use crate::{Artwork, ImageFormat, Media, bmff, codes, units};

/// The most bytes lofty may fetch from one file: tags and pictures at the start or the end, the
/// first frames, an Ogg stream's last page - and, for a raw AAC or WavPack stream, a header of
/// every frame or block it walks. A file that would make it read more (a raw AAC stream of more
/// than 32 MiB, a long search for a frame through junk) ends there with `too_large`.
pub const READ_BUDGET: u64 = 32 << 20;
/// The most lofty allocates for one tag item, a picture included.
pub const ALLOCATION_LIMIT: usize = 8 << 20;
/// The window lofty reads through: small, since it seeks from frame to frame, and read a byte at
/// a time while it looks for one.
const WINDOW: u32 = 16 << 10;
/// The most items one tag lists.
const MAX_ITEMS: usize = 1000;

/// The common fields, in the order a person looks for them, with their labels.
const COMMON: &[(ItemKey, &str)] = &[
    (ItemKey::TrackTitle, "Title"),
    (ItemKey::TrackArtist, "Artist"),
    (ItemKey::AlbumTitle, "Album"),
    (ItemKey::AlbumArtist, "Album artist"),
    (ItemKey::RecordingDate, "Date"),
    (ItemKey::Year, "Year"),
    (ItemKey::ReleaseDate, "Release date"),
    (ItemKey::TrackNumber, "Track"),
    (ItemKey::TrackTotal, "Tracks"),
    (ItemKey::DiscNumber, "Disc"),
    (ItemKey::Genre, "Genre"),
    (ItemKey::Comment, "Comment"),
];

/// Reads the audio file `src`; `format` is what its extension names, used when lofty cannot
/// tell the format from the bytes.
pub fn read<S: Source>(src: &mut S, format: Audio, want_cover: bool) -> Result<Media, Failure> {
    apply_global_options(
        GlobalOptions::new()
            .allocation_limit(ALLOCATION_LIMIT)
            .use_custom_resolvers(false)
            .preserve_format_specific_items(false),
    );
    let options = ParseOptions::new()
        .read_cover_art(want_cover)
        .parsing_mode(ParsingMode::BestAttempt);
    let mut reader = SourceReader::new(src, WINDOW, READ_BUDGET);
    let result = read_tagged(&mut reader, format, options);
    // lofty swallows some read errors and carries on; a cancel still ends the call.
    let failure = reader.take_failure();
    if let Some(cancel) = failure
        .as_ref()
        .filter(|failure| failure.kind == FailureKind::Cancelled)
    {
        return Err(cancel.clone());
    }
    let tagged = result.map_err(|error| failure.unwrap_or_else(|| classify(&error)))?;
    let mut media = describe(&tagged);
    // An MP4's codecs come from our own box reader; the rest lofty told already.
    if tagged.file_type() == FileType::Mp4
        && let Ok(video) = bmff::read(src, false)
    {
        media.sections.extend(
            video
                .sections
                .into_iter()
                .filter(|section| section.label == "Tracks"),
        );
    }
    Ok(media)
}

fn read_tagged<S: Source>(
    reader: &mut SourceReader<'_, S>,
    format: Audio,
    options: ParseOptions,
) -> Result<TaggedFile, FileParseError> {
    let mut probe = Probe::new(reader).options(options).guess_file_type()?;
    if probe.file_type().is_none() {
        probe = probe.set_file_type(file_type(format));
    }
    probe.read()
}

/// lofty's format for the extension's.
fn file_type(format: Audio) -> FileType {
    match format {
        Audio::Mpeg => FileType::Mpeg,
        Audio::Flac => FileType::Flac,
        Audio::Ogg => FileType::Vorbis,
        Audio::Mp4 => FileType::Mp4,
        Audio::Wav => FileType::Wav,
        Audio::Aiff => FileType::Aiff,
        Audio::Ape => FileType::Ape,
        Audio::WavPack => FileType::WavPack,
        Audio::Aac => FileType::Aac,
    }
}

/// A failure of lofty's: an allocation over the limit is `too_large`, anything else the file's.
fn classify(error: &FileParseError) -> Failure {
    let mut cause = error.source();
    while let Some(inner) = cause {
        if inner.is::<AllocationError>() {
            return Failure::bad(codes::TOO_LARGE);
        }
        cause = inner.source();
    }
    Failure::bad(codes::BAD_AUDIO)
}

/// The name of lofty's format for a person.
fn format_name(file_type: FileType) -> String {
    match file_type {
        FileType::Aac => "AAC (ADTS)".to_owned(),
        FileType::Aiff => "AIFF".to_owned(),
        FileType::Ape => "Monkey's Audio (APE)".to_owned(),
        FileType::Flac => "FLAC".to_owned(),
        FileType::Mpeg => "MPEG audio (MP3)".to_owned(),
        FileType::Mp4 => "MP4 audio".to_owned(),
        FileType::Mpc => "Musepack".to_owned(),
        FileType::Opus => "Opus (Ogg)".to_owned(),
        FileType::Vorbis => "Vorbis (Ogg)".to_owned(),
        FileType::Speex => "Speex (Ogg)".to_owned(),
        FileType::Wav => "WAV".to_owned(),
        FileType::WavPack => "WavPack".to_owned(),
        other => format!("{other:?}"),
    }
}

/// The name of a tag's format for a person.
fn tag_name(tag_type: TagType) -> String {
    match tag_type {
        TagType::Ape => "APE".to_owned(),
        TagType::Id3v1 => "ID3v1".to_owned(),
        TagType::Id3v2 => "ID3v2".to_owned(),
        TagType::Mp4Ilst => "MP4".to_owned(),
        TagType::VorbisComments => "Vorbis comments".to_owned(),
        TagType::RiffInfo => "RIFF INFO".to_owned(),
        TagType::AiffText => "AIFF text".to_owned(),
        other => format!("{other:?}"),
    }
}

/// The facts of a file lofty read.
fn describe(tagged: &TaggedFile) -> Media {
    let properties = tagged.properties();
    let mut audio = vec![Node::fact("Format", format_name(tagged.file_type()))];
    let duration = u64::try_from(properties.duration().as_millis()).unwrap_or(u64::MAX);
    if duration > 0 {
        audio.push(Node::fact("Duration", units::duration_millis(duration)));
    }
    if let Some(bitrate) = properties.audio_bitrate().filter(|rate| *rate > 0) {
        audio.push(Node::fact("Bitrate", format!("{bitrate} kbps")));
    }
    if let Some(bitrate) = properties.overall_bitrate().filter(|rate| *rate > 0) {
        audio.push(Node::fact("Overall bitrate", format!("{bitrate} kbps")));
    }
    if let Some(rate) = properties.sample_rate().filter(|rate| *rate > 0) {
        audio.push(Node::fact("Sample rate", format!("{rate} Hz")));
    }
    if let Some(channels) = properties.channels().filter(|count| *count > 0) {
        audio.push(Node::fact("Channels", units::channels(u64::from(channels))));
    }
    if let Some(depth) = properties.bit_depth().filter(|depth| *depth > 0) {
        audio.push(Node::fact("Bit depth", format!("{depth} bit")));
    }
    let mut media = Media {
        title: None,
        sections: vec![Node::group("Audio", None, audio)],
        artwork: None,
    };
    for tag in tagged.tags() {
        if media.title.is_none() {
            media.title = tag.get_string(ItemKey::TrackTitle).map(str::to_owned);
        }
        media.sections.push(describe_tag(tag));
        if !tag.pictures().is_empty() {
            media.sections.push(describe_pictures(tag));
        }
        if media.artwork.is_none() {
            media.artwork = artwork(tag.pictures());
        }
    }
    media
}

/// One tag: the common fields first, then every other item by its key.
fn describe_tag(tag: &Tag) -> Node {
    let mut facts = Vec::new();
    for (key, label) in COMMON {
        for value in tag
            .get_items(*key)
            .filter_map(|item| item_value(item.value()))
        {
            facts.push(Node::fact(*label, value));
        }
    }
    for item in tag.items() {
        if facts.len() >= MAX_ITEMS {
            break;
        }
        if COMMON.iter().any(|(key, _)| *key == item.key()) {
            continue;
        }
        if let Some(value) = item_value(item.value()) {
            facts.push(Node::fact(key_label(item.key()), value));
        }
    }
    Node::group(format!("Tags ({})", tag_name(tag.tag_type())), None, facts)
}

/// The pictures of one tag: type, format and size.
fn describe_pictures(tag: &Tag) -> Node {
    let facts = tag
        .pictures()
        .iter()
        .take(MAX_ITEMS)
        .map(|picture| {
            let format = ImageFormat::sniff(picture.data()).map_or_else(
                || "other format".to_owned(),
                |format| format.as_str().to_owned(),
            );
            Node::fact(
                picture_label(picture.pic_type()),
                format!("{format}, {} bytes", picture.data().len()),
            )
        })
        .collect();
    Node::group("Pictures", None, facts)
}

/// The cover to show: the front cover, else the first picture, as long as it is PNG or JPEG by
/// its bytes.
fn artwork(pictures: &[Picture]) -> Option<Artwork> {
    let usable = |picture: &&Picture| ImageFormat::sniff(picture.data()).is_some();
    let picture = pictures
        .iter()
        .filter(usable)
        .find(|picture| picture.pic_type() == PictureType::CoverFront)
        .or_else(|| pictures.iter().find(usable))?;
    Some(Artwork {
        format: ImageFormat::sniff(picture.data())?,
        bytes: picture.data().to_vec(),
    })
}

fn picture_label(pic_type: PictureType) -> String {
    match pic_type {
        PictureType::CoverFront => "Front cover".to_owned(),
        PictureType::CoverBack => "Back cover".to_owned(),
        PictureType::Other => "Picture".to_owned(),
        other => split_words(&format!("{other:?}")),
    }
}

/// An item's value as text; binary data by its size.
fn item_value(value: &ItemValue) -> Option<String> {
    match value {
        ItemValue::Text(text) | ItemValue::Locator(text) => {
            Some(text.clone()).filter(|text| !text.trim().is_empty())
        }
        ItemValue::Binary(bytes) => Some(format!("{} bytes", bytes.len())),
    }
}

/// A key's name for a person: `EncoderSoftware` as `Encoder software`.
fn key_label(key: ItemKey) -> String {
    split_words(&format!("{key:?}"))
}

/// `CamelCase` as `Camel case`.
fn split_words(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (index, c) in name.chars().enumerate() {
        if index > 0 && c.is_ascii_uppercase() {
            out.push(' ');
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
#[path = "audio_tests.rs"]
mod tests;
