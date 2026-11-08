//! rCommander's bundled PDF viewer (RC-100-99): the text of a PDF page by page (`text`, first)
//! and what the document says about itself (`tree`: information dictionary, page count, version,
//! encryption, page size), read with hayro 0.9 (pure Rust, MIT/Apache-2.0).
//!
//! - **Read whole, within a limit.** hayro parses a document in memory, so `open` reads the file
//!   through `read-at` in windows of 1 MiB, at most [`MAX_PDF_BYTES`] (the manifest's
//!   `max_size`), asking `should-stop` before each.
//! - **An empty user password opens, any other is a notice.** hayro tries the empty password;
//!   a document that needs another is `wrong-password` with the code `password_required`, an
//!   encryption hayro cannot read `not-supported`.
//! - **Nothing outside the file.** No JavaScript, no link, no embedded file is opened; fonts the
//!   document does not embed are hayro's bundled standard fonts.
//! - **Bounded text.** At most [`MAX_TEXT_PAGES`] pages and [`MAX_TEXT_BYTES`] of text; each
//!   page at most `layout::MAX_PAGE_BYTES`. Moving on cancels between pages.
//!
//! The layout is the other bundled viewers': the logic here, tested natively with `cargo test`;
//! the WIT translation in `src/guest.rs`, only on `wasm32`.

pub mod extract;
#[cfg(target_arch = "wasm32")]
mod guest;
pub mod info;
pub mod layout;
pub mod types;

#[cfg(test)]
mod test_support;

use hayro_interpret::hayro_syntax::{DecryptionError, LoadPdfError, Pdf};

use types::{Document, Failure, FailureKind, Output, Sink, Source, TreeNode};

/// The largest PDF read: hayro holds it whole, beside its parsed objects and fonts.
pub const MAX_PDF_BYTES: u64 = 64 << 20;
/// The most text written, as `plugins.viewer_output_limit_mib` allows at most.
pub const MAX_TEXT_BYTES: u64 = 256 << 20;
/// The most pages whose text is written.
pub const MAX_TEXT_PAGES: usize = 5000;
/// The window `open` reads through and `text` writes through.
const WINDOW: u32 = 1 << 20;
/// The bytes `probe` looks at for the `%PDF-` header.
const PROBE_BYTES: u64 = 1024;

/// The failure codes, under the plugin's id (`locales/en.json`).
pub mod code {
    pub const TOO_LARGE: &str = "01a1b720-fdd6-75dd-a577-079e819e4742.too_large";
    pub const DAMAGED: &str = "01a1b720-fdd6-75dd-a577-079e819e4742.damaged";
    pub const PASSWORD_REQUIRED: &str = "01a1b720-fdd6-75dd-a577-079e819e4742.password_required";
    pub const ENCRYPTION: &str = "01a1b720-fdd6-75dd-a577-079e819e4742.encryption";
}

/// Whether the file is a PDF: `%PDF-` within its first KiB, where readers look for it.
pub fn probe<S: Source>(src: &mut S) -> bool {
    let length = u32::try_from(src.size().min(PROBE_BYTES)).unwrap_or(1024);
    src.read_at(0, length)
        .is_ok_and(|head| head.windows(5).any(|window| window == b"%PDF-"))
}

/// One open document.
pub struct Viewer<S: Source> {
    src: S,
    pdf: Pdf,
    facts: Vec<(&'static str, String)>,
}

impl<S: Source> Viewer<S> {
    /// `open`: reads the file and its structure; offers `text`, then `tree`.
    pub fn open(mut src: S) -> Result<(Self, Document), Failure> {
        let bytes = read_all(&mut src)?;
        let encrypted = info::names_encryption(&bytes);
        let pdf = Pdf::new(bytes).map_err(failure_of)?;
        let facts = info::facts(&pdf, encrypted);
        let title = pdf
            .metadata()
            .title
            .as_deref()
            .map(info::text_string)
            .filter(|title| !title.is_empty());
        let viewer = Self { src, pdf, facts };
        Ok((
            viewer,
            Document {
                outputs: vec![Output::Text, Output::Tree],
                title,
                pages: None,
            },
        ))
    }

    /// The tree's facts: label and value.
    #[must_use]
    pub fn facts(&self) -> &[(&'static str, String)] {
        &self.facts
    }

    /// `text`: each page's text after a `Page n` line, a blank line between pages, as UTF-8.
    pub fn text(&mut self, out: &mut impl Sink) -> Result<String, Failure> {
        let total = self.pdf.pages().len();
        let mut writer = TextOut {
            out,
            offset: 0,
            failure: None,
        };
        let src = &mut self.src;
        extract::pages(&self.pdf, |index, text| {
            if src.should_stop() {
                writer.failure = Some(Failure::of(FailureKind::Cancelled));
                return false;
            }
            if index >= MAX_TEXT_PAGES {
                writer.write(&format!("[... {} more pages]\n", total - index));
                return false;
            }
            let heading = if index == 0 {
                "Page 1\n".to_owned()
            } else {
                format!("\nPage {}\n", index + 1)
            };
            writer.write(&heading) && writer.write(&text)
        });
        match writer.failure {
            Some(failure) => Err(failure),
            None => Ok("utf-8".to_owned()),
        }
    }

    /// `children`: the facts for `None`, as leaves; a leaf has no children.
    pub fn children(&mut self, node: Option<u64>) -> Result<Vec<TreeNode>, Failure> {
        if node.is_some() {
            return Ok(Vec::new());
        }
        Ok(self
            .facts
            .iter()
            .zip(1u64..)
            .map(|((label, value), id)| TreeNode {
                id,
                label: (*label).to_owned(),
                value: Some(value.clone()),
                has_children: false,
            })
            .collect())
    }
}

/// The text written in windows; it stops at [`MAX_TEXT_BYTES`] or the first failure.
struct TextOut<'o, O: Sink> {
    out: &'o mut O,
    offset: u64,
    failure: Option<Failure>,
}

impl<O: Sink> TextOut<'_, O> {
    /// Writes `text`; `false` when nothing more is to be written.
    fn write(&mut self, text: &str) -> bool {
        if self.failure.is_some() {
            return false;
        }
        let room = usize::try_from(MAX_TEXT_BYTES.saturating_sub(self.offset)).unwrap_or(0);
        let mut bytes = text.as_bytes();
        let full = bytes.len() > room;
        if full {
            let mut cut = room;
            while cut > 0 && !text.is_char_boundary(cut) {
                cut -= 1;
            }
            bytes = &bytes[..cut];
        }
        for chunk in bytes.chunks(WINDOW as usize) {
            if let Err(failure) = self.out.write_at(self.offset, chunk) {
                self.failure = Some(failure);
                return false;
            }
            self.offset += chunk.len() as u64;
        }
        !full
    }
}

/// The whole file, in windows, within [`MAX_PDF_BYTES`].
fn read_all<S: Source>(src: &mut S) -> Result<Vec<u8>, Failure> {
    let size = src.size();
    if size > MAX_PDF_BYTES {
        return Err(Failure::coded(FailureKind::BadData, code::TOO_LARGE));
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    while (bytes.len() as u64) < size {
        if src.should_stop() {
            return Err(Failure::of(FailureKind::Cancelled));
        }
        let wanted =
            u32::try_from((size - bytes.len() as u64).min(u64::from(WINDOW))).unwrap_or(WINDOW);
        let window = src.read_at(bytes.len() as u64, wanted)?;
        if window.is_empty() {
            break;
        }
        bytes.extend_from_slice(&window);
    }
    Ok(bytes)
}

fn failure_of(error: LoadPdfError) -> Failure {
    match error {
        LoadPdfError::Decryption(DecryptionError::PasswordProtected) => {
            Failure::coded(FailureKind::WrongPassword, code::PASSWORD_REQUIRED)
        }
        LoadPdfError::Decryption(_) => Failure::coded(FailureKind::NotSupported, code::ENCRYPTION),
        LoadPdfError::Invalid => Failure::coded(FailureKind::BadData, code::DAMAGED),
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
