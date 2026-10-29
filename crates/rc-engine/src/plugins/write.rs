//! A document's outputs written into files of the session (RC-100-29): the text, opened for
//! reading and its lines counted, and the picture, encoded or a raw RGBA raster. Each output has
//! a file of its own; `open.rs` writes the document's first one before the window shows it, the
//! document the other on the first switch to it (`document.rs`).

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use rc_plugin_host::{ImageFormat, ViewerSession, codes};
use rc_plugin_types::Failure;
use rc_vfs::VfsPath;
use tempfile::TempPath;

use super::{
    codes::failure_code,
    document::{PictureFormat, WrittenPicture, WrittenText},
    streams::{LineFeed, OutputSink},
};
use crate::{
    Engine,
    viewer::{Detected, FileWindow, LineIndex, STEP_BYTES, TextEncoding},
};

/// Where a step stopped: the flag was raised, or a failure's code.
pub(super) enum Stop {
    Cancelled,
    Failed(&'static str),
}

impl Stop {
    /// A failure the host answered; its cancellation is the person's, not the plugin's.
    pub(super) fn of(failure: &Failure) -> Self {
        match failure_code(failure.code()) {
            codes::PLUGIN_CANCELLED => Self::Cancelled,
            code => Self::Failed(code),
        }
    }

    /// The stable code the window shows.
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::Cancelled => codes::PLUGIN_CANCELLED,
            Self::Failed(code) => code,
        }
    }
}

/// A temporary file of the session, removed with the document.
fn output_file() -> Result<(std::fs::File, TempPath), Stop> {
    let file = tempfile::Builder::new()
        .prefix("rcommander-view-")
        .tempfile()
        .map_err(|error| {
            tracing::warn!(%error, "a viewer plugin's output file could not be created");
            Stop::Failed(codes::PLUGIN_SINK_FAILED)
        })?;
    Ok(file.into_parts())
}

/// The document's text written into a file of its own, opened for reading and its lines
/// counted.
pub(super) async fn text(
    engine: &Engine,
    session: &mut ViewerSession,
    cancel: &AtomicBool,
) -> Result<WrittenText, Stop> {
    let (file, path) = output_file()?;
    let feed = Arc::new(Mutex::new(LineFeed::default()));
    let sink = OutputSink::new(file, Arc::clone(&feed));
    let written = session
        .text(Box::new(sink))
        .await
        .map_err(|failure| Stop::of(&failure))?;
    let encoding = text_encoding(&written.encoding);
    let window = engine
        .open_for_view(&VfsPath::local(path.to_path_buf()))
        .await
        .map_err(|error| Stop::Failed(error.code()))?;
    let head = window
        .read_uncached(0, 4)
        .await
        .map_err(|error| Stop::Failed(error.code()))?;
    let detected = Detected::chosen(encoding, &head);
    let fed = feed.lock().ok().map(|mut feed| std::mem::take(&mut *feed));
    let index = match fed.and_then(|feed| feed.finish(written.bytes, encoding)) {
        Some(index) => index,
        None => count_lines(&window, detected, cancel).await?,
    };
    Ok(WrittenText {
        file: Arc::new(window),
        detected,
        index,
        path,
    })
}

/// The output counted line by line, for text written out of order or in UTF-16.
async fn count_lines(
    window: &FileWindow,
    detected: Detected,
    cancel: &AtomicBool,
) -> Result<LineIndex, Stop> {
    let mut index = LineIndex::new(detected);
    while !index
        .step(window, STEP_BYTES)
        .await
        .map_err(|error| Stop::Failed(error.code()))?
    {
        if cancel.load(Ordering::SeqCst) {
            return Err(Stop::Cancelled);
        }
    }
    Ok(index)
}

/// The document's picture written into a file of its own. The host holds it to the output
/// limit and a raw raster to the picture limits and to exactly `width * height * 4` bytes; the
/// length is checked here again, as the window reads the file by it.
pub(super) async fn picture(session: &mut ViewerSession) -> Result<WrittenPicture, Stop> {
    let (file, path) = output_file()?;
    let sink = OutputSink::new(file, Arc::new(Mutex::new(LineFeed::default())));
    let written = session
        .image(Box::new(sink))
        .await
        .map_err(|failure| Stop::of(&failure))?;
    let format = match written.format {
        ImageFormat::Rgba { width, height } => {
            let length = u64::from(width) * u64::from(height) * 4;
            if length != written.bytes {
                return Err(Stop::Failed(codes::PLUGIN_BAD_OUTPUT));
            }
            PictureFormat::Rgba { width, height }
        }
        _ => PictureFormat::Encoded,
    };
    Ok(WrittenPicture { format, path })
}

/// The viewer's encoding for the label `text` named; UTF-8 for one it does not read.
fn text_encoding(label: &str) -> TextEncoding {
    match label {
        "utf-16le" | "utf-16" => TextEncoding::Utf16Le,
        "utf-16be" => TextEncoding::Utf16Be,
        "iso-8859-1" | "latin1" | "latin-1" => TextEncoding::Latin1,
        "windows-1252" | "cp1252" => TextEncoding::Windows1252,
        "ibm437" | "cp437" => TextEncoding::Cp437,
        _ => TextEncoding::Utf8,
    }
}
