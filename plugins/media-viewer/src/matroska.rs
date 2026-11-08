//! Matroska and WebM, read element by element through `read-at`: the EBML header (`DocType`),
//! then the `Segment`'s children - `Info` (time scale, duration, title, the applications) and
//! `Tracks` (per `TrackEntry` number, type, codec, language, name, picture size or sampling
//! frequency and channels). Everything else is skipped by its size; a `Cluster` of unknown size
//! (a live recording) ends the walk, since it cannot be skipped without reading it. The walk stops
//! once both `Info` and `Tracks` are read.

use crate::ebml::{Element, Elements};
use crate::io::SourceReader;
use crate::outline::Node;
use crate::types::{Failure, Source};
use crate::{Media, codes, units};

const EBML: u32 = 0x1A45_DFA3;
const DOC_TYPE: u32 = 0x4282;
const SEGMENT: u32 = 0x1853_8067;
const INFO: u32 = 0x1549_A966;
const TRACKS: u32 = 0x1654_AE6B;
const TIMESTAMP_SCALE: u32 = 0x2A_D7B1;
const DURATION: u32 = 0x4489;
const TITLE: u32 = 0x7BA9;
const MUXING_APP: u32 = 0x4D80;
const WRITING_APP: u32 = 0x5741;
const TRACK_ENTRY: u32 = 0xAE;
const TRACK_NUMBER: u32 = 0xD7;
const TRACK_TYPE: u32 = 0x83;
const CODEC_ID: u32 = 0x86;
const LANGUAGE: u32 = 0x22_B59C;
const LANGUAGE_BCP47: u32 = 0x22_B59D;
const NAME: u32 = 0x536E;
const VIDEO: u32 = 0xE0;
const PIXEL_WIDTH: u32 = 0xB0;
const PIXEL_HEIGHT: u32 = 0xBA;
const AUDIO: u32 = 0xE1;
const SAMPLING_FREQUENCY: u32 = 0xB5;
const CHANNELS: u32 = 0x9F;
const BIT_DEPTH: u32 = 0x6264;

/// The most bytes one file may make the reader fetch.
const READ_BUDGET: u64 = 32 << 20;
/// The window elements are read through: small, since clusters are skipped.
const WINDOW: u32 = 4 << 10;
/// The most children of the `Segment` walked before `Info` and `Tracks` are found.
const MAX_SEGMENT_CHILDREN: usize = 65_536;
/// The most tracks described.
const MAX_TRACKS: usize = 256;

fn bad() -> Failure {
    Failure::bad(codes::BAD_VIDEO)
}

#[derive(Default)]
struct Track {
    number: Option<u64>,
    kind: Option<u64>,
    codec: Option<String>,
    name: Option<String>,
    language: Option<String>,
    size: Option<(u64, u64)>,
    sampling: Option<f64>,
    channels: Option<u64>,
    bit_depth: Option<u64>,
}

/// Reads the Matroska or WebM file `src`: the container and its tracks.
pub fn read<S: Source>(src: &mut S) -> Result<Media, Failure> {
    let mut reader = SourceReader::new(src, WINDOW, READ_BUDGET);
    let size = reader.size();
    let mut elements = Elements::new(&mut reader);
    let header = elements
        .header(0, size)?
        .filter(|element| element.id == EBML)
        .ok_or_else(bad)?;
    let mut doc_type = None;
    for child in elements.children(&header)? {
        if child.id == DOC_TYPE {
            doc_type = Some(elements.string(&child)?);
        }
    }
    let mut at = header.known_end()?;
    let segment = loop {
        let element = elements.header(at, size)?.ok_or_else(bad)?;
        if element.id == SEGMENT {
            break element;
        }
        at = element.known_end()?;
    };
    let segment_end = segment.end.unwrap_or(size);
    let mut file = vec![Node::fact(
        "Container",
        match doc_type.as_deref() {
            Some("webm") => "WebM".to_owned(),
            Some("matroska") | None => "Matroska".to_owned(),
            Some(other) => format!("Matroska ({other})"),
        },
    )];
    let mut title = None;
    let mut tracks: Option<Vec<Track>> = None;
    let mut info_read = false;
    let mut at = segment.body;
    for _ in 0..MAX_SEGMENT_CHILDREN {
        let Some(child) = elements.header(at, segment_end)? else {
            break;
        };
        match child.id {
            INFO => {
                title = read_info(&mut elements, &child, &mut file)?;
                info_read = true;
            }
            TRACKS => tracks = Some(read_tracks(&mut elements, &child)?),
            _ => {}
        }
        if info_read && tracks.is_some() {
            break;
        }
        match child.end {
            Some(end) => at = end,
            // A cluster of unknown size cannot be skipped; nor can anything else of one.
            None => break,
        }
    }
    let mut sections = vec![Node::group("File", None, file)];
    if let Some(tracks) = tracks.filter(|tracks| !tracks.is_empty()) {
        let nodes = tracks
            .iter()
            .enumerate()
            .map(|(index, track)| describe(index, track));
        sections.push(Node::group("Tracks", None, nodes.collect()));
    }
    Ok(Media {
        title,
        sections,
        artwork: None,
    })
}

/// `Info`: its facts go into `file`; the title is returned.
fn read_info<S: Source>(
    elements: &mut Elements<'_, '_, S>,
    info: &Element,
    file: &mut Vec<Node>,
) -> Result<Option<String>, Failure> {
    let mut scale = 1_000_000;
    let mut duration = None;
    let mut title = None;
    let mut apps = Vec::new();
    for child in elements.children(info)? {
        match child.id {
            TIMESTAMP_SCALE => scale = elements.uint(&child)?,
            DURATION => duration = Some(elements.float(&child)?),
            TITLE => title = Some(elements.string(&child)?).filter(|text| !text.is_empty()),
            MUXING_APP => apps.push(Node::fact("Muxing application", elements.string(&child)?)),
            WRITING_APP => apps.push(Node::fact("Writing application", elements.string(&child)?)),
            _ => {}
        }
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "a time scale is nanoseconds per tick; a float is what Matroska multiplies it by"
    )]
    let seconds = duration.map(|ticks| ticks * scale as f64 / 1e9);
    if let Some(text) = seconds.and_then(units::duration_seconds) {
        file.push(Node::fact("Duration", text));
    }
    if let Some(text) = &title {
        file.push(Node::fact("Title", text.clone()));
    }
    file.extend(apps);
    Ok(title)
}

fn read_tracks<S: Source>(
    elements: &mut Elements<'_, '_, S>,
    tracks: &Element,
) -> Result<Vec<Track>, Failure> {
    let mut found = Vec::new();
    for entry in elements.children(tracks)? {
        if entry.id != TRACK_ENTRY || found.len() >= MAX_TRACKS {
            continue;
        }
        let mut track = Track::default();
        for child in elements.children(&entry)? {
            match child.id {
                TRACK_NUMBER => track.number = Some(elements.uint(&child)?),
                TRACK_TYPE => track.kind = Some(elements.uint(&child)?),
                CODEC_ID => track.codec = Some(elements.string(&child)?),
                NAME => track.name = Some(elements.string(&child)?),
                LANGUAGE if track.language.is_none() => {
                    track.language = Some(elements.string(&child)?);
                }
                // The BCP 47 tag wins over the older ISO 639-2 one.
                LANGUAGE_BCP47 => track.language = Some(elements.string(&child)?),
                VIDEO => read_video(elements, &child, &mut track)?,
                AUDIO => read_audio(elements, &child, &mut track)?,
                _ => {}
            }
        }
        found.push(track);
    }
    Ok(found)
}

fn read_video<S: Source>(
    elements: &mut Elements<'_, '_, S>,
    video: &Element,
    track: &mut Track,
) -> Result<(), Failure> {
    let (mut width, mut height) = (None, None);
    for child in elements.children(video)? {
        match child.id {
            PIXEL_WIDTH => width = Some(elements.uint(&child)?),
            PIXEL_HEIGHT => height = Some(elements.uint(&child)?),
            _ => {}
        }
    }
    if let (Some(width), Some(height)) = (width, height) {
        track.size = Some((width, height));
    }
    Ok(())
}

fn read_audio<S: Source>(
    elements: &mut Elements<'_, '_, S>,
    audio: &Element,
    track: &mut Track,
) -> Result<(), Failure> {
    // Matroska's defaults: 8000 Hz, one channel.
    track.sampling = Some(8000.0);
    track.channels = Some(1);
    for child in elements.children(audio)? {
        match child.id {
            SAMPLING_FREQUENCY => track.sampling = Some(elements.float(&child)?),
            CHANNELS => track.channels = Some(elements.uint(&child)?),
            BIT_DEPTH => track.bit_depth = Some(elements.uint(&child)?),
            _ => {}
        }
    }
    Ok(())
}

fn describe(index: usize, track: &Track) -> Node {
    let mut facts = Vec::new();
    if let Some(codec) = &track.codec {
        facts.push(Node::fact("Codec", units::matroska_codec(codec)));
    }
    if let Some(name) = track.name.as_ref().filter(|name| !name.is_empty()) {
        facts.push(Node::fact("Name", name.clone()));
    }
    if let Some((width, height)) = track.size {
        facts.push(Node::fact("Resolution", format!("{width} x {height}")));
    }
    if let Some(channels) = track.channels {
        facts.push(Node::fact("Channels", units::channels(channels)));
    }
    if let Some(rate) = track
        .sampling
        .filter(|rate| rate.is_finite() && *rate > 0.0)
    {
        facts.push(Node::fact("Sample rate", format!("{rate} Hz")));
    }
    if let Some(depth) = track.bit_depth {
        facts.push(Node::fact("Bit depth", format!("{depth} bit")));
    }
    // A track without a language is English, by Matroska's default.
    let language = track.language.clone().unwrap_or_else(|| "eng".to_owned());
    facts.push(Node::fact("Language", language));
    let number = track.number.unwrap_or(index as u64 + 1);
    let kind = match track.kind {
        Some(1) => "Video".to_owned(),
        Some(2) => "Audio".to_owned(),
        Some(3) => "Complex".to_owned(),
        Some(0x10) => "Logo".to_owned(),
        Some(0x11) => "Subtitles".to_owned(),
        Some(0x12) => "Buttons".to_owned(),
        Some(0x20) => "Control".to_owned(),
        Some(0x21) => "Metadata".to_owned(),
        Some(other) => format!("Type {other}"),
        None => "Unknown".to_owned(),
    };
    Node::group(format!("Track {number}"), Some(kind), facts)
}

#[cfg(test)]
#[path = "matroska_tests.rs"]
mod tests;
