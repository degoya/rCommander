//! How the readers write numbers and codes for a person: durations, channel counts, box types
//! and the common codec names.

/// `ms` as `m:ss.mmm`, or `h:mm:ss.mmm` from an hour on.
#[must_use]
pub fn duration_millis(ms: u64) -> String {
    let hours = ms / 3_600_000;
    let minutes = (ms / 60_000) % 60;
    let seconds = (ms / 1000) % 60;
    let millis = ms % 1000;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}.{millis:03}")
    } else {
        format!("{minutes}:{seconds:02}.{millis:03}")
    }
}

/// `seconds` as [`duration_millis`] does; `None` for a value no file means (negative, not a
/// number, or longer than ten thousand years).
#[must_use]
pub fn duration_seconds(seconds: f64) -> Option<String> {
    const MAX_SECONDS: f64 = 3.2e11;
    if !seconds.is_finite() || !(0.0..=MAX_SECONDS).contains(&seconds) {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is finite, non-negative and below 2^53 milliseconds"
    )]
    let ms = (seconds * 1000.0).round() as u64;
    Some(duration_millis(ms))
}

/// `units` of a duration counted in `scale` units per second.
#[must_use]
pub fn duration_scaled(units: u64, scale: u64) -> Option<String> {
    if scale == 0 {
        return None;
    }
    let ms = u128::from(units) * 1000 / u128::from(scale);
    u64::try_from(ms).ok().map(duration_millis)
}

/// A channel count, with its usual name.
#[must_use]
pub fn channels(count: u64) -> String {
    match count {
        1 => "1 (mono)".to_owned(),
        2 => "2 (stereo)".to_owned(),
        6 => "6 (5.1)".to_owned(),
        8 => "8 (7.1)".to_owned(),
        other => other.to_string(),
    }
}

/// A box type or item name: printable ASCII as it is, `0xA9` as the copyright sign, anything else
/// as `?`.
#[must_use]
pub fn fourcc(kind: &[u8]) -> String {
    kind.iter()
        .map(|byte| match byte {
            0xA9 => '\u{a9}',
            0x20..=0x7E => char::from(*byte),
            _ => '?',
        })
        .collect()
}

/// The codec of an MP4 sample entry, by its type: `H.264/AVC (avc1)`.
#[must_use]
pub fn bmff_codec(kind: &[u8]) -> String {
    let name = match kind {
        b"avc1" | b"avc3" => "H.264/AVC",
        b"hvc1" | b"hev1" => "H.265/HEVC",
        b"av01" => "AV1",
        b"vp08" => "VP8",
        b"vp09" => "VP9",
        b"mp4v" => "MPEG-4 Visual",
        b"s263" => "H.263",
        b"mjp2" | b"jpeg" => "Motion JPEG",
        b"apch" | b"apcn" | b"apcs" | b"apco" | b"ap4h" => "Apple ProRes",
        b"mp4a" => "AAC",
        b"alac" => "Apple Lossless",
        b"fLaC" => "FLAC",
        b"Opus" => "Opus",
        b"ac-3" => "AC-3",
        b"ec-3" => "E-AC-3",
        b"samr" => "AMR",
        b"sawb" => "AMR-WB",
        b".mp3" => "MP3",
        b"tx3g" => "Timed text",
        b"wvtt" => "WebVTT",
        b"c608" => "CEA-608 captions",
        b"tmcd" => "Timecode",
        _ => return fourcc(kind),
    };
    format!("{name} ({})", fourcc(kind))
}

/// The codec of a Matroska track, by its `CodecID`: `H.264/AVC (V_MPEG4/ISO/AVC)`.
#[must_use]
pub fn matroska_codec(id: &str) -> String {
    let name = match id {
        "V_MPEG4/ISO/AVC" => "H.264/AVC",
        "V_MPEGH/ISO/HEVC" => "H.265/HEVC",
        "V_AV1" => "AV1",
        "V_VP8" => "VP8",
        "V_VP9" => "VP9",
        "V_MPEG4/ISO/ASP" | "V_MPEG4/ISO/SP" => "MPEG-4 Visual",
        "V_MPEG2" => "MPEG-2",
        "V_THEORA" => "Theora",
        "A_AAC" => "AAC",
        "A_OPUS" => "Opus",
        "A_VORBIS" => "Vorbis",
        "A_FLAC" => "FLAC",
        "A_MPEG/L3" => "MP3",
        "A_AC3" => "AC-3",
        "A_EAC3" => "E-AC-3",
        "A_DTS" => "DTS",
        "A_TRUEHD" => "Dolby TrueHD",
        "S_TEXT/UTF8" => "SubRip text",
        "S_TEXT/ASS" | "S_TEXT/SSA" => "SubStation Alpha",
        "S_TEXT/WEBVTT" => "WebVTT",
        "S_HDMV/PGS" => "PGS subtitles",
        "S_VOBSUB" => "VobSub",
        _ if id.starts_with("A_PCM") => "PCM",
        _ if id.starts_with("A_AAC/") => "AAC",
        _ => return id.to_owned(),
    };
    format!("{name} ({id})")
}

#[cfg(test)]
#[path = "units_tests.rs"]
mod tests;
