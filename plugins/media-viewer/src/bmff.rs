//! ISO base media files (MP4, M4V, M4A, QuickTime, 3GP), read box by box through `read-at`.
//!
//! The top level is walked header by header - `ftyp`, `mdat` of many GiB skipped by its size -
//! until `moov`, before or after the media data. Inside `moov` (at most [`MAX_MOOV`]) only the
//! boxes named below are entered and only the heads of the small leaves are read: `mvhd` (the
//! duration), per `trak` `tkhd`, `mdia` > `mdhd` (time scale, duration, language) and `hdlr` (the
//! track's kind), `minf` > `stbl` > `stsd` (the first sample entry: codec, picture size or
//! channels and sample rate); the tags of `udta` and `meta` are [`crate::bmff_items`]'s. The
//! sample tables, the reason a `moov` grows, are skipped.
//!
//! Every size is checked against what is left of the parent box and of the file, with
//! `checked_add`: a box that does not fit, a `largesize` of `u64::MAX`, a size below its own
//! header all end in `bad-data` ([`codes::BAD_VIDEO`]). The walk is bounded in depth (a fixed path,
//! at most seven boxes deep) and in boxes visited ([`MAX_BOXES`]).

use crate::bmff_items::{self, Items};
use crate::io::{SourceReader, be_u16, be_u32, be_u64};
use crate::outline::Node;
use crate::types::{Failure, Source};
use crate::{Media, codes, units};

/// The largest `moov` the viewer reads.
pub const MAX_MOOV: u64 = 64 << 20;
/// The most bytes one file may make the reader fetch: headers, the heads of the leaves and a
/// cover.
const READ_BUDGET: u64 = 32 << 20;
/// The window headers are read through: small, since they are scattered.
const WINDOW: u32 = 4 << 10;
/// The most boxes visited in one file.
pub const MAX_BOXES: u32 = 100_000;
/// The most children listed of one box.
const MAX_CHILDREN: usize = 4096;
/// The most tracks described.
const MAX_TRACKS: usize = 256;

/// One box: its type and where it and its body are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxHeader {
    pub kind: [u8; 4],
    /// The first byte after the header.
    pub body: u64,
    /// The first byte after the box.
    pub end: u64,
}

/// A walker of boxes over a [`SourceReader`], counting the boxes it visits.
pub struct Boxes<'r, 'a, S: Source> {
    pub reader: &'r mut SourceReader<'a, S>,
    visited: u32,
}

impl<'r, 'a, S: Source> Boxes<'r, 'a, S> {
    pub fn new(reader: &'r mut SourceReader<'a, S>) -> Self {
        Self { reader, visited: 0 }
    }

    /// The box at `at` inside a parent that ends at `end`; `None` when fewer than eight bytes are
    /// left (a QuickTime list may end in four zero bytes).
    pub fn header(&mut self, at: u64, end: u64) -> Result<Option<BoxHeader>, Failure> {
        if end.saturating_sub(at) < 8 {
            return Ok(None);
        }
        self.visited += 1;
        if self.visited > MAX_BOXES {
            return Err(Failure::bad(codes::BAD_VIDEO));
        }
        let head = self.reader.head(at, 16)?;
        let bad = || Failure::bad(codes::BAD_VIDEO);
        let size32 = be_u32(&head, 0).ok_or_else(bad)?;
        let mut kind = [0u8; 4];
        kind.copy_from_slice(head.get(4..8).ok_or_else(bad)?);
        let (header_len, size) = match size32 {
            0 => (8, end - at),
            1 => (16, be_u64(&head, 8).ok_or_else(bad)?),
            size => (8, u64::from(size)),
        };
        if size < header_len {
            return Err(bad());
        }
        let box_end = at.checked_add(size).ok_or_else(bad)?;
        if box_end > end {
            return Err(bad());
        }
        Ok(Some(BoxHeader {
            kind,
            body: at + header_len,
            end: box_end,
        }))
    }

    /// The children of the body `start..end`, at most [`MAX_CHILDREN`].
    pub fn children(&mut self, start: u64, end: u64) -> Result<Vec<BoxHeader>, Failure> {
        let mut children = Vec::new();
        let mut at = start;
        while children.len() < MAX_CHILDREN {
            let Some(child) = self.header(at, end)? else {
                break;
            };
            at = child.end;
            children.push(child);
        }
        Ok(children)
    }

    /// The first child of type `kind` in `start..end`.
    pub fn child(
        &mut self,
        start: u64,
        end: u64,
        kind: &[u8; 4],
    ) -> Result<Option<BoxHeader>, Failure> {
        Ok(self
            .children(start, end)?
            .into_iter()
            .find(|child| &child.kind == kind))
    }

    /// At most `len` bytes of the body of `header`.
    pub fn head(&mut self, header: &BoxHeader, len: usize) -> Result<Vec<u8>, Failure> {
        let available = usize::try_from(header.end - header.body).unwrap_or(usize::MAX);
        self.reader.head(header.body, len.min(available))
    }
}

/// One track of `moov`.
#[derive(Default)]
struct Track {
    id: Option<u32>,
    handler: Option<[u8; 4]>,
    codec: Option<[u8; 4]>,
    size: Option<(u32, u32)>,
    channels: Option<u16>,
    sample_rate: Option<u32>,
    duration: Option<String>,
    language: Option<String>,
}

/// Reads the ISO base media file `src`: the container, its tracks and its tags; the cover as
/// artwork when `want_cover`.
pub fn read<S: Source>(src: &mut S, want_cover: bool) -> Result<Media, Failure> {
    let mut reader = SourceReader::new(src, WINDOW, READ_BUDGET);
    let size = reader.size();
    let mut boxes = Boxes::new(&mut reader);
    let mut brand = None;
    let mut moov = None;
    let mut at = 0;
    while let Some(header) = boxes.header(at, size)? {
        match &header.kind {
            b"ftyp" => brand = Some(boxes.head(&header, 4)?),
            b"moov" => {
                moov = Some(header);
                break;
            }
            _ => {}
        }
        at = header.end;
    }
    let moov = moov.ok_or_else(|| Failure::bad(codes::NO_MOOV))?;
    if moov.end - moov.body > MAX_MOOV {
        return Err(Failure::bad(codes::TOO_LARGE));
    }
    let mut file = Vec::new();
    let container = match brand.as_deref() {
        Some(brand) if brand.len() == 4 => {
            format!("ISO base media (brand {})", units::fourcc(brand))
        }
        _ => "QuickTime".to_owned(),
    };
    file.push(Node::fact("Container", container));
    let mut tracks = Vec::new();
    let mut items = Items::default();
    for child in boxes.children(moov.body, moov.end)? {
        match &child.kind {
            b"mvhd" => {
                if let Some(duration) = movie_duration(&boxes.head(&child, 32)?) {
                    file.push(Node::fact("Duration", duration));
                }
            }
            b"trak" if tracks.len() < MAX_TRACKS => tracks.push(read_trak(&mut boxes, &child)?),
            b"udta" => bmff_items::read_udta(&mut boxes, &child, want_cover, &mut items)?,
            b"meta" => bmff_items::read_meta(&mut boxes, &child, want_cover, &mut items)?,
            _ => {}
        }
    }
    let mut sections = vec![Node::group("File", None, file)];
    if !tracks.is_empty() {
        let nodes = tracks
            .iter()
            .enumerate()
            .map(|(index, track)| describe(index, track));
        sections.push(Node::group("Tracks", None, nodes.collect()));
    }
    if !items.facts.is_empty() {
        sections.push(Node::group("Tags", None, items.facts));
    }
    Ok(Media {
        title: items.title,
        sections,
        artwork: items.artwork,
    })
}

/// The duration of `mvhd`: version, flags, then the times, the time scale and the duration.
fn movie_duration(head: &[u8]) -> Option<String> {
    let (scale, duration) = match head.first()? {
        1 => (be_u32(head, 20)?, be_u64(head, 24)?),
        _ => (be_u32(head, 12)?, u64::from(be_u32(head, 16)?)),
    };
    if duration == u64::MAX || duration == u64::from(u32::MAX) {
        return None;
    }
    units::duration_scaled(duration, u64::from(scale))
}

fn read_trak<S: Source>(boxes: &mut Boxes<'_, '_, S>, trak: &BoxHeader) -> Result<Track, Failure> {
    let mut track = Track::default();
    for child in boxes.children(trak.body, trak.end)? {
        match &child.kind {
            b"tkhd" => read_tkhd(&boxes.head(&child, 96)?, &mut track),
            b"mdia" => read_mdia(boxes, &child, &mut track)?,
            _ => {}
        }
    }
    Ok(track)
}

/// `tkhd`: the track id and, as a fallback, the picture size (16.16 fixed point).
fn read_tkhd(head: &[u8], track: &mut Track) {
    let (id_at, size_at) = match head.first() {
        Some(1) => (20, 88),
        _ => (12, 76),
    };
    track.id = be_u32(head, id_at);
    if let (Some(width), Some(height)) = (be_u32(head, size_at), be_u32(head, size_at + 4))
        && width >> 16 > 0
        && height >> 16 > 0
    {
        track.size = Some((width >> 16, height >> 16));
    }
}

fn read_mdia<S: Source>(
    boxes: &mut Boxes<'_, '_, S>,
    mdia: &BoxHeader,
    track: &mut Track,
) -> Result<(), Failure> {
    for child in boxes.children(mdia.body, mdia.end)? {
        match &child.kind {
            b"mdhd" => read_mdhd(&boxes.head(&child, 40)?, track),
            b"hdlr" => {
                let head = boxes.head(&child, 12)?;
                if let Some(kind) = head.get(8..12) {
                    let mut handler = [0u8; 4];
                    handler.copy_from_slice(kind);
                    track.handler = Some(handler);
                }
            }
            b"minf" => {
                let Some(stbl) = boxes.child(child.body, child.end, b"stbl")? else {
                    continue;
                };
                if let Some(stsd) = boxes.child(stbl.body, stbl.end, b"stsd")? {
                    read_stsd(&boxes.head(&stsd, 64)?, track);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// `mdhd`: the track's time scale, duration and language.
fn read_mdhd(head: &[u8], track: &mut Track) {
    let (scale, duration, language) = match head.first() {
        Some(1) => (be_u32(head, 20), be_u64(head, 24), be_u16(head, 32)),
        _ => (
            be_u32(head, 12),
            be_u32(head, 16).map(u64::from),
            be_u16(head, 20),
        ),
    };
    if let (Some(scale), Some(duration)) = (scale, duration)
        && duration != u64::MAX
        && duration != u64::from(u32::MAX)
    {
        track.duration = units::duration_scaled(duration, u64::from(scale));
    }
    track.language = language.and_then(language_code);
}

/// The ISO 639-2 code packed into fifteen bits, three letters of five bits each; a QuickTime
/// language number (below `0x400`) or anything but letters is none.
fn language_code(packed: u16) -> Option<String> {
    if packed < 0x400 {
        return None;
    }
    let letters = [(packed >> 10) & 0x1F, (packed >> 5) & 0x1F, packed & 0x1F];
    letters
        .iter()
        .map(|letter| {
            u8::try_from(letter + 0x60)
                .ok()
                .filter(u8::is_ascii_lowercase)
                .map(char::from)
        })
        .collect()
}

/// `stsd`: version and flags, the entry count, then the first sample entry - its size, type,
/// six reserved bytes and the data reference index, then the visual or audio fields.
fn read_stsd(head: &[u8], track: &mut Track) {
    let Some(kind) = head.get(12..16) else {
        return;
    };
    let mut codec = [0u8; 4];
    codec.copy_from_slice(kind);
    track.codec = Some(codec);
    match track.handler.as_ref() {
        Some(b"vide") => {
            if let (Some(width), Some(height)) = (be_u16(head, 40), be_u16(head, 42))
                && width > 0
                && height > 0
            {
                track.size = Some((u32::from(width), u32::from(height)));
            }
        }
        // A QuickTime version 2 entry keeps its rate elsewhere; it is left out.
        Some(b"soun") if be_u16(head, 24).is_some_and(|version| version < 2) => {
            track.channels = be_u16(head, 32).filter(|count| *count > 0);
            track.sample_rate = be_u32(head, 40)
                .map(|rate| rate >> 16)
                .filter(|rate| *rate > 0);
        }
        _ => {}
    }
}

fn describe(index: usize, track: &Track) -> Node {
    let mut facts = Vec::new();
    if let Some(codec) = track.codec {
        facts.push(Node::fact("Codec", units::bmff_codec(&codec)));
    }
    if let Some((width, height)) = track.size.filter(|_| track.handler != Some(*b"soun")) {
        facts.push(Node::fact("Resolution", format!("{width} x {height}")));
    }
    if let Some(channels) = track.channels {
        facts.push(Node::fact("Channels", units::channels(u64::from(channels))));
    }
    if let Some(rate) = track.sample_rate {
        facts.push(Node::fact("Sample rate", format!("{rate} Hz")));
    }
    if let Some(duration) = &track.duration {
        facts.push(Node::fact("Duration", duration.clone()));
    }
    if let Some(language) = &track.language {
        facts.push(Node::fact("Language", language.clone()));
    }
    let number = track.id.map_or(index as u64 + 1, u64::from);
    let kind = match track.handler.as_ref() {
        Some(b"vide") => "Video".to_owned(),
        Some(b"soun") => "Audio".to_owned(),
        Some(b"text" | b"sbtl" | b"subt" | b"clcp") => "Subtitles".to_owned(),
        Some(b"tmcd") => "Timecode".to_owned(),
        Some(b"meta" | b"mdta") => "Metadata".to_owned(),
        Some(b"hint") => "Hint".to_owned(),
        Some(other) => units::fourcc(other),
        None => "Unknown".to_owned(),
    };
    Node::group(format!("Track {number}"), Some(kind), facts)
}

#[cfg(test)]
#[path = "bmff_tests.rs"]
mod tests;
