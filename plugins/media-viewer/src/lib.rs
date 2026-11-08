//! rCommander's bundled media viewer (RC-100-99): what an audio file, a video file or a photo
//! says about itself - duration, codecs, tracks, tags, the embedded cover, the EXIF of TIFF, HEIC
//! and camera raw files - as a tree (`tree`, first) and as `label: value` lines (`text`), and the
//! cover as a picture (`image`) when there is one and the setting `cover` is `on`.
//!
//! What the host promises a viewer, and how this one is built on it (`docs/plugins.md`, "Viewer
//! (WLX)"):
//!
//! - **The file is a handle, read in windows, never whole.** Media files are many GiB; every
//!   reader here goes through [`io::SourceReader`], which fetches windows through `read-at` and
//!   at most a **budget** of bytes in all, so neither our readers nor lofty's nor kamadak-exif's
//!   can walk a whole file. The manifest names no `max_size`.
//! - **Every size from the file is distrusted.** The video readers ([`bmff`], [`matroska`]) check
//!   each box and element size against what is left of its parent and of the file, with
//!   `checked_add`; a size that does not fit ends in `bad-data`, never in an overflow or a large
//!   allocation. Only small leaves are read into memory; `mdat` and Matroska clusters are skipped
//!   by their size.
//! - **Everything is read in `open`.** The document holds the facts (a small tree, capped in
//!   [`outline`]) and the cover; `text`, `image` and `children` only hand them out.
//! - **Moving on cancels.** Every window asks `should-stop` first.
//!
//! The layout is rDownloader's for plugins (`sdk/templates/*`, commit `26c8592a6`): the logic
//! here, tested natively with `cargo test`; the WIT translation in `src/guest.rs`, only on
//! `wasm32`. The types in [`types`] mirror the viewer world one to one for that.

pub mod audio;
mod bmff;
mod bmff_items;
mod ebml;
#[cfg(target_arch = "wasm32")]
mod guest;
mod io;
pub mod kind;
mod matroska;
pub mod outline;
mod photo;
pub mod settings;
pub mod types;
mod units;

#[cfg(test)]
mod fixtures;
#[cfg(test)]
mod fixtures_video;
#[cfg(test)]
mod test_support;

use kind::Kind;
use outline::{Node, Outline};
use settings::{Cover, Settings};
use types::{Document, Failure, FailureKind, Output, Sink, Source, TreeNode};

/// The failure codes of `locales/en.json`, under the plugin's id.
pub mod codes {
    /// The audio file is damaged or not the format its extension names.
    pub const BAD_AUDIO: &str = "01a1b720-fdcc-7b78-bca0-da328630ad19.bad_audio";
    /// A box or element of a video file is cut off, or larger than its parent or the file.
    pub const BAD_VIDEO: &str = "01a1b720-fdcc-7b78-bca0-da328630ad19.bad_video";
    /// An MP4 file without a movie header (`moov`).
    pub const NO_MOOV: &str = "01a1b720-fdcc-7b78-bca0-da328630ad19.no_moov";
    /// A part the viewer must read is larger than it reads, or the file asks for more reading
    /// than the budget allows.
    pub const TOO_LARGE: &str = "01a1b720-fdcc-7b78-bca0-da328630ad19.too_large";
    /// No EXIF data within reach.
    pub const NO_EXIF: &str = "01a1b720-fdcc-7b78-bca0-da328630ad19.no_exif";
    /// The EXIF data is damaged.
    pub const BAD_EXIF: &str = "01a1b720-fdcc-7b78-bca0-da328630ad19.bad_exif";
    /// Neither the extension nor the first bytes name a format the viewer reads.
    pub const NOT_MEDIA: &str = "01a1b720-fdcc-7b78-bca0-da328630ad19.not_media";
}

/// The formats an embedded cover is passed on in, unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageFormat {
    Png,
    Jpeg,
}

impl ImageFormat {
    /// The format of `bytes` by their signature; `None` for anything but PNG and JPEG.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else {
            None
        }
    }

    /// The name `image` answers.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
        }
    }
}

/// An embedded cover, as the file holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artwork {
    pub format: ImageFormat,
    pub bytes: Vec<u8>,
}

/// What one reader found.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Media {
    /// The document's title: a tag's or the container's.
    pub title: Option<String>,
    pub sections: Vec<Node>,
    pub artwork: Option<Artwork>,
}

/// The first bytes of `src`, for [`Kind::sniff`] and [`Kind::matches`].
fn read_head<S: Source>(src: &mut S) -> Result<Vec<u8>, Failure> {
    let length = u32::try_from(src.size().min(u64::from(kind::HEAD_BYTES))).unwrap_or(0);
    if length == 0 {
        return Ok(Vec::new());
    }
    let mut head = src.read_at(0, length)?;
    head.truncate(length as usize);
    Ok(head)
}

/// The reader for `src`: its extension's, else its first bytes'.
fn kind_of<S: Source>(src: &S, head: &[u8]) -> Option<Kind> {
    Kind::from_name(&src.name()).or_else(|| Kind::sniff(head))
}

/// `probe`: whether the file starts as the format its extension names (or, without a known
/// extension, as any format the viewer reads).
pub fn probe<S: Source>(src: &mut S) -> bool {
    let Ok(head) = read_head(src) else {
        return false;
    };
    match Kind::from_name(&src.name()) {
        Some(kind) => kind.matches(&head),
        None => Kind::sniff(&head).is_some(),
    }
}

/// One open document: its facts and its cover.
pub struct Viewer {
    outline: Outline,
    title: Option<String>,
    artwork: Option<Artwork>,
}

impl Viewer {
    /// `open`: reads what the file says about itself and offers `tree`, `text` and, with a cover
    /// and the setting `cover` on, `image`.
    pub fn open<S: Source>(src: &mut S, settings: Settings) -> Result<(Self, Document), Failure> {
        let head = read_head(src)?;
        let kind = kind_of(src, &head)
            .ok_or_else(|| Failure::coded(FailureKind::NotSupported, codes::NOT_MEDIA))?;
        let want_cover = settings.cover == Cover::On;
        let media = match kind {
            Kind::Audio(format) => audio::read(src, format, want_cover)?,
            Kind::Bmff => bmff::read(src, want_cover)?,
            Kind::Matroska => matroska::read(src)?,
            Kind::Tiff => photo::read_tiff(src)?,
            Kind::Heif => photo::read_heif(src)?,
        };
        let viewer = Self {
            outline: Outline::new(media.sections),
            title: media.title,
            artwork: if want_cover { media.artwork } else { None },
        };
        let document = viewer.document();
        Ok((viewer, document))
    }

    /// The outputs this document offers, `tree` first.
    #[must_use]
    pub fn document(&self) -> Document {
        let mut outputs = vec![Output::Tree, Output::Text];
        if self.artwork.is_some() {
            outputs.push(Output::Image);
        }
        Document {
            outputs,
            title: self.title.clone(),
            pages: None,
        }
    }

    /// The facts as text, as `text` writes them.
    #[must_use]
    pub fn text_string(&self) -> String {
        self.outline.render()
    }

    /// `text`: the facts as `label: value` lines into `out`, as UTF-8.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        outline::write_all(out, self.outline.render().as_bytes())?;
        Ok("utf-8".to_owned())
    }

    /// `image`: the embedded cover into `out`, unchanged, and its format.
    pub fn image(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        let artwork = self
            .artwork
            .as_ref()
            .ok_or_else(|| Failure::of(FailureKind::NotSupported))?;
        outline::write_all(out, &artwork.bytes)?;
        Ok(artwork.format.as_str().to_owned())
    }

    /// `children`: the sections for `None`, else the children of the node with that id.
    pub fn children(&self, node: Option<u64>) -> Result<Vec<TreeNode>, Failure> {
        self.outline.children(node)
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
