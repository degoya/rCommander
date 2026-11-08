//! Which reader a file goes to: by its extension (the manifest's `[match] extensions`), else by
//! its first bytes; and `probe`'s check that the bytes are the format the extension names.

/// The audio formats lofty reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Audio {
    Mpeg,
    Flac,
    Ogg,
    Mp4,
    Wav,
    Aiff,
    Ape,
    WavPack,
    Aac,
}

/// The reader a file goes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Audio(Audio),
    /// ISO base media: MP4, M4V, QuickTime, 3GP.
    Bmff,
    /// Matroska and WebM.
    Matroska,
    /// TIFF and the camera raw formats built on it.
    Tiff,
    /// HEIF and HEIC photos.
    Heif,
}

/// How many bytes of the head [`Kind::sniff`] and [`Kind::matches`] look at.
pub const HEAD_BYTES: u32 = 64;

const EBML_MAGIC: &[u8] = &[0x1A, 0x45, 0xDF, 0xA3];
const HEIF_BRANDS: &[&[u8; 4]] = &[
    b"heic", b"heix", b"heim", b"heis", b"hevc", b"hevx", b"mif1", b"msf1", b"avif", b"avis",
];
const AUDIO_BRANDS: &[&[u8; 4]] = &[b"M4A ", b"M4B ", b"M4P "];
/// The box types a QuickTime file without `ftyp` may start with.
const BMFF_FIRST_BOXES: &[&[u8; 4]] = &[
    b"ftyp", b"moov", b"mdat", b"free", b"skip", b"wide", b"pnot", b"uuid",
];

impl Kind {
    /// The reader for the extension of `name`; `None` for any other.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let (_, extension) = name.rsplit_once('.')?;
        Some(match extension.to_ascii_lowercase().as_str() {
            "mp3" => Self::Audio(Audio::Mpeg),
            "flac" => Self::Audio(Audio::Flac),
            "ogg" | "opus" => Self::Audio(Audio::Ogg),
            "m4a" => Self::Audio(Audio::Mp4),
            "wav" => Self::Audio(Audio::Wav),
            "aiff" | "aif" => Self::Audio(Audio::Aiff),
            "ape" => Self::Audio(Audio::Ape),
            "wv" => Self::Audio(Audio::WavPack),
            "aac" => Self::Audio(Audio::Aac),
            "mp4" | "m4v" | "mov" | "3gp" => Self::Bmff,
            "mkv" | "webm" => Self::Matroska,
            "tif" | "tiff" | "dng" | "cr2" | "nef" | "arw" => Self::Tiff,
            "heic" | "heif" => Self::Heif,
            _ => return None,
        })
    }

    /// The reader for a file whose name says nothing, by its first bytes.
    #[must_use]
    pub fn sniff(head: &[u8]) -> Option<Self> {
        if head.starts_with(EBML_MAGIC) {
            return Some(Self::Matroska);
        }
        if is_tiff(head) {
            return Some(Self::Tiff);
        }
        if head.get(4..8) == Some(b"ftyp".as_slice())
            && let Some(brand) = head.get(8..12)
        {
            if HEIF_BRANDS.iter().any(|known| known.as_slice() == brand) {
                return Some(Self::Heif);
            }
            if AUDIO_BRANDS.iter().any(|known| known.as_slice() == brand) {
                return Some(Self::Audio(Audio::Mp4));
            }
            return Some(Self::Bmff);
        }
        if is_bmff(head) {
            return Some(Self::Bmff);
        }
        // MPEG first: an ID3 tag may lead any of them, and lofty finds what follows it.
        let audio = [
            Audio::Mpeg,
            Audio::Flac,
            Audio::Ogg,
            Audio::Wav,
            Audio::Aiff,
            Audio::Ape,
            Audio::WavPack,
            Audio::Aac,
        ];
        audio
            .into_iter()
            .find(|format| audio_matches(*format, head))
            .map(Self::Audio)
    }

    /// `probe`: whether `head` starts as the format of this reader does.
    #[must_use]
    pub fn matches(self, head: &[u8]) -> bool {
        match self {
            Self::Audio(format) => audio_matches(format, head),
            Self::Bmff => is_bmff(head),
            Self::Matroska => head.starts_with(EBML_MAGIC),
            Self::Tiff => is_tiff(head),
            Self::Heif => head.get(4..8) == Some(b"ftyp".as_slice()),
        }
    }
}

fn is_tiff(head: &[u8]) -> bool {
    head.starts_with(b"II*\0") || head.starts_with(b"MM\0*")
}

fn is_bmff(head: &[u8]) -> bool {
    head.get(4..8).is_some_and(|kind| {
        BMFF_FIRST_BOXES
            .iter()
            .any(|known| known.as_slice() == kind)
    })
}

/// An MPEG audio frame sync with a valid layer.
fn is_mpeg_sync(head: &[u8]) -> bool {
    matches!(head, [0xFF, second, ..] if *second & 0xE0 == 0xE0 && (*second >> 1) & 0b11 != 0)
}

/// An ADTS (AAC) frame sync: layer `00`.
fn is_adts_sync(head: &[u8]) -> bool {
    matches!(head, [0xFF, second, ..] if *second & 0xF6 == 0xF0)
}

fn audio_matches(format: Audio, head: &[u8]) -> bool {
    let id3 = head.starts_with(b"ID3");
    match format {
        // Leading zero bytes are padding lofty skips.
        Audio::Mpeg => id3 || is_mpeg_sync(head.get(first_non_zero(head)..).unwrap_or_default()),
        Audio::Aac => id3 || is_adts_sync(head),
        Audio::Flac => id3 || head.starts_with(b"fLaC"),
        Audio::Ogg => head.starts_with(b"OggS"),
        Audio::Mp4 => head.get(4..8) == Some(b"ftyp".as_slice()),
        Audio::Wav => head.starts_with(b"RIFF") && head.get(8..12) == Some(b"WAVE".as_slice()),
        Audio::Aiff => {
            head.starts_with(b"FORM") && matches!(head.get(8..12), Some(b"AIFF") | Some(b"AIFC"))
        }
        Audio::Ape => id3 || head.starts_with(b"MAC "),
        Audio::WavPack => head.starts_with(b"wvpk"),
    }
}

fn first_non_zero(head: &[u8]) -> usize {
    head.iter()
        .position(|byte| *byte != 0)
        .unwrap_or(head.len())
}

#[cfg(test)]
#[path = "kind_tests.rs"]
mod tests;
