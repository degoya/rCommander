//! Small files built byte by byte for the tests: an MP3 with an ID3v2.4 tag, a FLAC with tags and
//! a picture, and TIFFs with EXIF and GPS in either byte order. Each is the least lofty 0.25 and
//! kamadak-exif 0.6 accept; the MP4, M4A and Matroska builders are in `fixtures_video.rs`.

/// The eight bytes of the PNG signature and a few more: what a cover starts with.
pub(crate) const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x02\0\0\0";
/// The start of a JPEG.
pub(crate) const JPEG: &[u8] = b"\xFF\xD8\xFF\xE0\0\x10JFIF\0\x01\x01\0\0\x01\0\x01\0\0\xFF\xD9";

/// A synchsafe integer: seven bits per byte.
fn synchsafe(value: u32) -> [u8; 4] {
    [
        ((value >> 21) & 0x7F) as u8,
        ((value >> 14) & 0x7F) as u8,
        ((value >> 7) & 0x7F) as u8,
        (value & 0x7F) as u8,
    ]
}

/// An ID3v2.4 text frame in UTF-8.
fn id3_text(id: &[u8; 4], text: &str) -> Vec<u8> {
    let mut content = vec![3u8];
    content.extend_from_slice(text.as_bytes());
    let mut frame = id.to_vec();
    frame.extend_from_slice(&synchsafe(content.len() as u32));
    frame.extend_from_slice(&[0, 0]);
    frame.extend_from_slice(&content);
    frame
}

/// An MP3: an ID3v2.4 tag (title, artist, album, track) and four MPEG-1 Layer III frames of
/// 128 kbps at 44.1 kHz, stereo (417 bytes each); the first carries an `Info` header that counts
/// 100 frames, so the duration is 100 * 1152 / 44100 s = 2.612 s.
pub(crate) fn mp3() -> Vec<u8> {
    let mut frames = Vec::new();
    frames.extend(id3_text(b"TIT2", "Song title"));
    frames.extend(id3_text(b"TPE1", "The Artist"));
    frames.extend(id3_text(b"TALB", "The Album"));
    frames.extend(id3_text(b"TRCK", "3"));
    let mut out = b"ID3".to_vec();
    out.extend_from_slice(&[4, 0, 0]);
    out.extend_from_slice(&synchsafe(frames.len() as u32));
    out.extend(frames);
    for index in 0..4 {
        let mut frame = vec![0u8; 417];
        frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
        if index == 0 {
            // After the header and 32 bytes of side information.
            frame[36..40].copy_from_slice(b"Info");
            frame[40..44].copy_from_slice(&[0, 0, 0, 3]);
            frame[44..48].copy_from_slice(&100u32.to_be_bytes());
            frame[48..52].copy_from_slice(&41_700u32.to_be_bytes());
        }
        out.extend(frame);
    }
    out
}

/// A FLAC metadata block: its type (the high bit marks the last) and a 24-bit size.
fn flac_block(kind: u8, last: bool, content: &[u8]) -> Vec<u8> {
    let size = content.len() as u32;
    let mut block = vec![kind | if last { 0x80 } else { 0 }];
    block.extend_from_slice(&size.to_be_bytes()[1..]);
    block.extend_from_slice(content);
    block
}

/// A FLAC: STREAMINFO (44.1 kHz, two channels, 16 bit, 441000 samples: ten seconds), a
/// VORBIS_COMMENT with title, artist and album, a PICTURE (front cover, PNG) and a few bytes of
/// audio.
pub(crate) fn flac() -> Vec<u8> {
    let mut info = Vec::new();
    info.extend_from_slice(&4096u16.to_be_bytes());
    info.extend_from_slice(&4096u16.to_be_bytes());
    info.extend_from_slice(&[0; 6]);
    let packed: u32 = (44_100 << 12) | (1 << 9) | (15 << 4);
    info.extend_from_slice(&packed.to_be_bytes());
    info.extend_from_slice(&441_000u32.to_be_bytes());
    info.extend_from_slice(&[0; 16]);

    let mut comments = Vec::new();
    let vendor = b"fixture";
    comments.extend_from_slice(&(vendor.len() as u32).to_le_bytes());
    comments.extend_from_slice(vendor);
    let fields = ["TITLE=Flac title", "ARTIST=Flac artist", "ALBUM=Flac album"];
    comments.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    for field in fields {
        comments.extend_from_slice(&(field.len() as u32).to_le_bytes());
        comments.extend_from_slice(field.as_bytes());
    }

    let mut picture = Vec::new();
    picture.extend_from_slice(&3u32.to_be_bytes());
    let mime = b"image/png";
    picture.extend_from_slice(&(mime.len() as u32).to_be_bytes());
    picture.extend_from_slice(mime);
    picture.extend_from_slice(&0u32.to_be_bytes());
    for value in [1u32, 1, 24, 0] {
        picture.extend_from_slice(&value.to_be_bytes());
    }
    picture.extend_from_slice(&(PNG.len() as u32).to_be_bytes());
    picture.extend_from_slice(PNG);

    let mut out = b"fLaC".to_vec();
    out.extend(flac_block(0, false, &info));
    out.extend(flac_block(4, false, &comments));
    out.extend(flac_block(6, true, &picture));
    out.extend_from_slice(&[0xFF, 0xF8, 0x69, 0x08]);
    out.extend_from_slice(&[0; 200]);
    out
}

/// A value of a TIFF field.
pub(crate) enum TiffValue {
    Ascii(&'static str),
    Long(u32),
    Rationals(Vec<(u32, u32)>),
}

/// A TIFF in either byte order, its IFDs laid out one after the other and the values that do not
/// fit four bytes after them.
struct TiffWriter {
    little: bool,
    data_at: u32,
    data: Vec<u8>,
}

impl TiffWriter {
    fn u16(&self, value: u16) -> [u8; 2] {
        if self.little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        }
    }

    fn u32(&self, value: u32) -> [u8; 4] {
        if self.little {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        }
    }

    /// One IFD of `entries`, then a zero next-IFD offset.
    fn ifd(&mut self, entries: &[(u16, TiffValue)]) -> Vec<u8> {
        let mut out = self.u16(entries.len() as u16).to_vec();
        for (tag, value) in entries {
            let (kind, count, bytes) = match value {
                TiffValue::Ascii(text) => {
                    let mut bytes = text.as_bytes().to_vec();
                    bytes.push(0);
                    (2u16, bytes.len() as u32, bytes)
                }
                TiffValue::Long(number) => (4, 1, self.u32(*number).to_vec()),
                TiffValue::Rationals(parts) => {
                    let mut bytes = Vec::new();
                    for (num, denom) in parts {
                        bytes.extend_from_slice(&self.u32(*num));
                        bytes.extend_from_slice(&self.u32(*denom));
                    }
                    (5, parts.len() as u32, bytes)
                }
            };
            out.extend_from_slice(&self.u16(*tag));
            out.extend_from_slice(&self.u16(kind));
            out.extend_from_slice(&self.u32(count));
            if bytes.len() <= 4 {
                let mut inline = bytes.clone();
                inline.resize(4, 0);
                out.extend_from_slice(&inline);
            } else {
                let offset = self.data_at + self.data.len() as u32;
                out.extend_from_slice(&self.u32(offset));
                self.data.extend_from_slice(&bytes);
            }
        }
        out.extend_from_slice(&self.u32(0));
        out
    }
}

/// The size of an IFD of `entries` entries.
fn ifd_len(entries: usize) -> u32 {
    2 + 12 * entries as u32 + 4
}

/// A TIFF whose first IFD names make and model and points to an Exif IFD (time taken, f-number)
/// and a GPS IFD (48.14 N, 11.571667 E).
pub(crate) fn tiff(little: bool) -> Vec<u8> {
    let exif_at = 8 + ifd_len(4);
    let gps_at = exif_at + ifd_len(2);
    let data_at = gps_at + ifd_len(4);
    let mut writer = TiffWriter {
        little,
        data_at,
        data: Vec::new(),
    };
    let ifd0 = writer.ifd(&[
        (0x010F, TiffValue::Ascii("Canon")),
        (0x0110, TiffValue::Ascii("EOS R5")),
        (0x8769, TiffValue::Long(exif_at)),
        (0x8825, TiffValue::Long(gps_at)),
    ]);
    let exif = writer.ifd(&[
        (0x9003, TiffValue::Ascii("2024:05:01 12:34:56")),
        (0x829D, TiffValue::Rationals(vec![(28, 10)])),
    ]);
    let gps = writer.ifd(&[
        (0x0001, TiffValue::Ascii("N")),
        (
            0x0002,
            TiffValue::Rationals(vec![(48, 1), (8, 1), (2400, 100)]),
        ),
        (0x0003, TiffValue::Ascii("E")),
        (
            0x0004,
            TiffValue::Rationals(vec![(11, 1), (34, 1), (1800, 100)]),
        ),
    ]);
    let mut out = if little {
        b"II*\0".to_vec()
    } else {
        b"MM\0*".to_vec()
    };
    out.extend_from_slice(&writer.u32(8));
    out.extend(ifd0);
    out.extend(exif);
    out.extend(gps);
    out.extend_from_slice(&writer.data);
    out
}
