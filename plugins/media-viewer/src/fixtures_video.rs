//! ISO base media and Matroska files built byte by byte for the tests: an M4A lofty 0.25 reads
//! (an audio track and an iTunes list), an MP4 with a video and an audio track, tags, a cover and
//! a phone's location (`moov` before or after `mdat`), and a Matroska file with `Info` and two
//! tracks.

use crate::fixtures::JPEG;

/// A box: its 32-bit size, its type and its body.
pub(crate) fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

/// A box of the boxes `children`.
pub(crate) fn container(kind: &[u8; 4], children: &[Vec<u8>]) -> Vec<u8> {
    bx(kind, &children.concat())
}

fn be32(value: u32) -> [u8; 4] {
    value.to_be_bytes()
}

/// `ftyp` of `brand`.
pub(crate) fn ftyp(brand: &[u8; 4]) -> Vec<u8> {
    let mut body = brand.to_vec();
    body.extend_from_slice(&be32(0));
    body.extend_from_slice(brand);
    body.extend_from_slice(b"isom");
    bx(b"ftyp", &body)
}

/// `mvhd`, version 0: a duration of `duration` in `scale` units per second.
pub(crate) fn mvhd(scale: u32, duration: u32) -> Vec<u8> {
    let mut body = vec![0u8; 100];
    body[12..16].copy_from_slice(&be32(scale));
    body[16..20].copy_from_slice(&be32(duration));
    body[20..24].copy_from_slice(&be32(0x0001_0000));
    bx(b"mvhd", &body)
}

/// `tkhd`, version 0, of track `id` with a picture size (0 for sound).
fn tkhd(id: u32, width: u32, height: u32) -> Vec<u8> {
    let mut body = vec![0u8; 84];
    body[0..4].copy_from_slice(&be32(7));
    body[12..16].copy_from_slice(&be32(id));
    body[76..80].copy_from_slice(&be32(width << 16));
    body[80..84].copy_from_slice(&be32(height << 16));
    bx(b"tkhd", &body)
}

/// `mdhd`, version 0, with a packed ISO 639-2 language.
fn mdhd(scale: u32, duration: u32, language: u16) -> Vec<u8> {
    let mut body = vec![0u8; 24];
    body[12..16].copy_from_slice(&be32(scale));
    body[16..20].copy_from_slice(&be32(duration));
    body[20..22].copy_from_slice(&language.to_be_bytes());
    bx(b"mdhd", &body)
}

/// `hdlr` of `handler`.
fn hdlr(handler: &[u8; 4]) -> Vec<u8> {
    let mut body = vec![0u8; 8];
    body.extend_from_slice(handler);
    body.extend_from_slice(&[0; 12]);
    body.extend_from_slice(b"Handler\0");
    bx(b"hdlr", &body)
}

/// `stsd` with one sample entry.
fn stsd(entry: Vec<u8>) -> Vec<u8> {
    let mut body = be32(0).to_vec();
    body.extend_from_slice(&be32(1));
    body.extend(entry);
    bx(b"stsd", &body)
}

/// An audio sample entry: six reserved bytes, the data reference, version, revision, vendor,
/// channels, sample size, two more fields and the rate as 16.16.
fn audio_entry(kind: &[u8; 4], channels: u16, rate: u32) -> Vec<u8> {
    let mut body = vec![0u8; 28];
    body[6..8].copy_from_slice(&1u16.to_be_bytes());
    body[16..18].copy_from_slice(&channels.to_be_bytes());
    body[18..20].copy_from_slice(&16u16.to_be_bytes());
    body[24..28].copy_from_slice(&be32(rate << 16));
    bx(kind, &body)
}

/// A visual sample entry of `width` by `height`.
fn video_entry(kind: &[u8; 4], width: u16, height: u16) -> Vec<u8> {
    let mut body = vec![0u8; 78];
    body[6..8].copy_from_slice(&1u16.to_be_bytes());
    body[24..26].copy_from_slice(&width.to_be_bytes());
    body[26..28].copy_from_slice(&height.to_be_bytes());
    bx(kind, &body)
}

/// A track: `tkhd`, then `mdia` with `mdhd`, `hdlr` and `minf` > `stbl` > `stsd` last, as lofty
/// expects.
fn trak(
    id: u32,
    size: (u32, u32),
    media: (u32, u32, u16),
    handler: &[u8; 4],
    entry: Vec<u8>,
) -> Vec<u8> {
    let stbl = container(b"stbl", &[stsd(entry)]);
    let minf = container(b"minf", &[stbl]);
    let mdia = container(
        b"mdia",
        &[mdhd(media.0, media.1, media.2), hdlr(handler), minf],
    );
    container(b"trak", &[tkhd(id, size.0, size.1), mdia])
}

/// An iTunes item with a `data` box of `data_type`.
pub(crate) fn item(kind: &[u8; 4], data_type: u32, value: &[u8]) -> Vec<u8> {
    let mut data = be32(data_type).to_vec();
    data.extend_from_slice(&be32(0));
    data.extend_from_slice(value);
    container(kind, &[bx(b"data", &data)])
}

/// `udta` > `meta` (a full box: version and flags, then `hdlr` and `ilst`, last) of `items`.
fn itunes(items: &[Vec<u8>], extra: &[Vec<u8>]) -> Vec<u8> {
    let mut handler = vec![0u8; 8];
    handler.extend_from_slice(b"mdirappl");
    handler.extend_from_slice(&[0; 9]);
    let mut meta = be32(0).to_vec();
    meta.extend(bx(b"hdlr", &handler));
    meta.extend(container(b"ilst", items));
    let mut children = extra.to_vec();
    children.push(bx(b"meta", &meta));
    container(b"udta", &children)
}

/// The language `eng` packed into fifteen bits.
pub(crate) const ENG: u16 = (5 << 10) | (14 << 5) | 7;
/// The language `und`.
pub(crate) const UND: u16 = (21 << 10) | (14 << 5) | 4;

/// An M4A lofty reads: AAC, two channels at 44.1 kHz, five seconds, title, artist and album.
pub(crate) fn m4a() -> Vec<u8> {
    let track = trak(
        1,
        (0, 0),
        (44_100, 220_500, UND),
        b"soun",
        audio_entry(b"mp4a", 2, 44_100),
    );
    let tags = itunes(
        &[
            item(b"\xa9nam", 1, b"M4A title"),
            item(b"\xa9ART", 1, b"M4A artist"),
            item(b"\xa9alb", 1, b"M4A album"),
        ],
        &[],
    );
    let moov = container(b"moov", &[mvhd(1000, 5000), track, tags]);
    [ftyp(b"M4A "), moov, bx(b"mdat", &[0; 100])].concat()
}

/// The `moov` of an MP4 from a phone: H.264 1920 x 1080 and AAC, ten seconds, a title, a JPEG
/// cover and the location QuickTime writes into `udta`.
pub(crate) fn mp4_moov() -> Vec<u8> {
    let video = trak(
        1,
        (1920, 1080),
        (90_000, 900_000, ENG),
        b"vide",
        video_entry(b"avc1", 1920, 1080),
    );
    let audio = trak(
        2,
        (0, 0),
        (48_000, 480_000, ENG),
        b"soun",
        audio_entry(b"mp4a", 2, 48_000),
    );
    let mut location = 18u16.to_be_bytes().to_vec();
    location.extend_from_slice(&ENG.to_be_bytes());
    location.extend_from_slice(b"+48.1374+011.5755/");
    let tags = itunes(
        &[item(b"\xa9nam", 1, b"Holiday"), item(b"covr", 13, JPEG)],
        &[bx(b"\xa9xyz", &location)],
    );
    container(b"moov", &[mvhd(1000, 10_000), video, audio, tags])
}

/// An MP4 with `moov` before `mdat`, or after it.
pub(crate) fn mp4(moov_first: bool) -> Vec<u8> {
    let mdat = bx(b"mdat", &[0x55; 1000]);
    if moov_first {
        [ftyp(b"isom"), mp4_moov(), mdat].concat()
    } else {
        [ftyp(b"isom"), mdat, mp4_moov()].concat()
    }
}

/// An EBML element: its ID (the marker is part of it) and a size of one, two or eight bytes.
pub(crate) fn el(id: u32, body: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = id
        .to_be_bytes()
        .into_iter()
        .skip_while(|byte| *byte == 0)
        .collect();
    let size = body.len() as u64;
    if size < 0x7F {
        out.push(0x80 | size as u8);
    } else if size < 0x3FFF {
        out.extend_from_slice(&(0x4000 | size as u16).to_be_bytes());
    } else {
        out.push(0x01);
        out.extend_from_slice(&size.to_be_bytes()[1..]);
    }
    out.extend_from_slice(body);
    out
}

/// An unsigned integer element, in as few bytes as it takes.
pub(crate) fn el_uint(id: u32, value: u64) -> Vec<u8> {
    let bytes: Vec<u8> = value
        .to_be_bytes()
        .into_iter()
        .skip_while(|byte| *byte == 0)
        .collect();
    el(id, &bytes)
}

/// A float element of eight bytes.
pub(crate) fn el_float(id: u32, value: f64) -> Vec<u8> {
    el(id, &value.to_bits().to_be_bytes())
}

/// The EBML header of `doc_type`.
pub(crate) fn ebml_header(doc_type: &str) -> Vec<u8> {
    el(
        0x1A45_DFA3,
        &[
            el_uint(0x4286, 1),
            el(0x4282, doc_type.as_bytes()),
            el_uint(0x4287, 4),
        ]
        .concat(),
    )
}

/// `Info`: a millisecond time scale, 12.345 s, a title and the applications.
pub(crate) fn mkv_info() -> Vec<u8> {
    el(
        0x1549_A966,
        &[
            el_uint(0x2A_D7B1, 1_000_000),
            el_float(0x4489, 12_345.0),
            el(0x7BA9, b"Holiday"),
            el(0x4D80, b"libebml"),
            el(0x5741, b"mkvmerge"),
        ]
        .concat(),
    )
}

/// `Tracks`: H.264 1280 x 720, and Opus in German at 48 kHz, two channels.
pub(crate) fn mkv_tracks() -> Vec<u8> {
    let video = el(
        0xAE,
        &[
            el_uint(0xD7, 1),
            el_uint(0x83, 1),
            el(0x86, b"V_MPEG4/ISO/AVC"),
            el(0x22_B59C, b"und"),
            el(0xE0, &[el_uint(0xB0, 1280), el_uint(0xBA, 720)].concat()),
        ]
        .concat(),
    );
    let audio = el(
        0xAE,
        &[
            el_uint(0xD7, 2),
            el_uint(0x83, 2),
            el(0x86, b"A_OPUS"),
            el(0x22_B59C, b"ger"),
            el(0x22_B59D, b"de"),
            el(0xE1, &[el_float(0xB5, 48_000.0), el_uint(0x9F, 2)].concat()),
        ]
        .concat(),
    );
    el(0x1654_AE6B, &[video, audio].concat())
}

/// A cluster of a few bytes of nothing.
pub(crate) fn mkv_cluster() -> Vec<u8> {
    el(
        0x1F43_B675,
        &[el_uint(0xE7, 0), el(0xA3, &[0; 300])].concat(),
    )
}

/// A Matroska file: the header, then a `Segment` of `children` (of known size).
pub(crate) fn mkv(children: &[Vec<u8>]) -> Vec<u8> {
    [ebml_header("matroska"), el(0x1853_8067, &children.concat())].concat()
}
