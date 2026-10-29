//! What the viewer reads, off the UI thread: a file opened (its head sniffed), a page of lines
//! or rows from a byte offset, a picture (a file's, or a plugin's raw raster), a search. Every
//! read goes through the engine's [`FileWindow`], so nothing ever holds more of a file than a
//! page and its block cache - only a picture is read whole, below [`picture::MAX_FILE_BYTES`].
//!
//! Errors are the stable codes the status line translates (`error.<code>`).

use std::sync::{Arc, atomic::AtomicBool};

use iced::{
    futures::{SinkExt, Stream, channel::mpsc},
    widget::image::Handle,
};
use rc_db::Db;
use rc_engine::{
    Engine, VfsPath,
    settings::{PICTURE_MEGAPIXELS_KEY, stored_setting},
    viewer::{
        self, DETECT_BYTES, Detected, Direction, FileWindow, ImageFormat, LineEnding, Match,
        Needle, SearchOutcome, TextEncoding, TextLine, ViewMode, encoding, hex, sniff, text,
    },
};

use super::last_page::LastPage;
use super::picture;

/// Bytes per row of the binary view.
pub const BINARY_COLUMNS: usize = 80;

/// A file opened for viewing.
#[derive(Debug, Clone)]
pub struct Opened {
    pub file: Arc<FileWindow>,
    /// Where its last text page starts, kept while it is open.
    pub last_page: LastPage,
    /// Its first bytes, for the byte-order mark of an encoding chosen by hand.
    pub head: Vec<u8>,
    pub detected: Detected,
    pub ending: Option<LineEnding>,
    pub image: Option<ImageFormat>,
    pub mode: ViewMode,
    /// Its extension is one of `viewer.dos_extensions`: CP437 unless its bytes say otherwise,
    /// and shown without wrapping (RC-020-11).
    pub dos: bool,
}

/// How a page is shown: everything a page load needs besides where it starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub mode: ViewMode,
    pub detected: Detected,
    pub tab_width: usize,
    /// Characters cut off the left of each text line (no wrap, scrolled right).
    pub left: usize,
    pub rows: usize,
}

/// Where the next page starts, from the current one's start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Seek {
    Stay,
    /// The line or row holding this byte.
    At(u64),
    Down(usize),
    Up(usize),
    End,
    /// A share of the file's size, from the scrollbar.
    Fraction(f32),
}

/// A part of the page, as line and byte column: the match to mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub from: (usize, usize),
    pub to: (usize, usize),
}

/// A page as the view shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub top: u64,
    /// Where the byte after the page is.
    pub end: u64,
    pub lines: Vec<String>,
    pub mark: Option<Span>,
    /// The text view's lines as read, each with its start: what a document's tokens are counted
    /// in (RC-100-10). Empty for the binary and hex views.
    pub raw: Vec<TextLine>,
}

/// A decoded picture, ready for the image widget.
#[derive(Debug, Clone)]
pub struct Picture {
    pub handle: Handle,
    pub width: u32,
    pub height: u32,
    /// What it was read from, for the status line: `PNG`, or `RGBA` for a raw raster.
    pub format: &'static str,
}

/// A search's news: bytes searched so far, then its end.
#[derive(Debug, Clone)]
pub enum Found {
    Progress(u64),
    Done(Result<SearchOutcome, &'static str>),
}

/// `path` opened through the engine, its head read to tell encoding, line ending and image; a
/// file `viewer.dos_extensions` names (read from `db`) is DOS text.
pub async fn open(engine: Engine, path: VfsPath, db: Option<Db>) -> Result<Opened, &'static str> {
    let file = engine.open_for_view(&path).await.map_err(|e| e.code())?;
    let head = file
        .read_uncached(0, DETECT_BYTES)
        .await
        .map_err(|e| e.code())?;
    let dos = viewer::is_dos_file(&path.path, &viewer::dos_extensions(db.as_ref()).await);
    let detected = if dos {
        viewer::detect_dos(&head)
    } else {
        viewer::detect(&head)
    };
    Ok(Opened {
        dos,
        ending: text::detect_line_ending(&head, detected),
        image: sniff::sniff_image(&head),
        mode: sniff::suggest_mode(&head),
        head: head.iter().take(4).copied().collect(),
        detected,
        file: Arc::new(file),
        last_page: LastPage::default(),
    })
}

/// The page of `layout` that `seek` leads to from `top`, with `mark` marked when it is on it.
pub async fn page(
    (file, last_page): (Arc<FileWindow>, LastPage),
    layout: Layout,
    top: u64,
    seek: Seek,
    mark: Option<Match>,
) -> Result<Page, &'static str> {
    match layout.mode {
        ViewMode::Text => text_page((&file, &last_page), layout, top, seek, mark).await,
        ViewMode::Hex | ViewMode::Binary => rows_page(&file, layout, top, seek, mark).await,
        ViewMode::Image => Ok(Page {
            top,
            end: top,
            lines: Vec::new(),
            mark: None,
            raw: Vec::new(),
        }),
    }
}

async fn text_page(
    (file, last_page): (&FileWindow, &LastPage),
    layout: Layout,
    top: u64,
    seek: Seek,
    mark: Option<Match>,
) -> Result<Page, &'static str> {
    let detected = layout.detected;
    let rows = layout.rows.max(1);
    // Read only for a seek that needs it, and once per layout (RC-080-53).
    let last = match seek {
        Seek::Stay | Seek::At(_) | Seek::Up(_) => 0,
        Seek::Down(_) | Seek::End | Seek::Fraction(_) => last_page.of(file, detected, rows).await?,
    };
    let top = match seek {
        Seek::Stay => top,
        Seek::At(offset) => text::line_start(file, detected, offset)
            .await
            .map_err(|e| e.code())?,
        Seek::Down(_) if top >= last => top,
        Seek::Down(count) => {
            let ahead = text::read_lines(file, detected, top, count + 1)
                .await
                .map_err(|e| e.code())?;
            ahead
                .lines
                .get(count)
                .map_or(last, |line| line.offset)
                .min(last)
        }
        Seek::Up(count) => text::lines_before(file, detected, top, count)
            .await
            .map_err(|e| e.code())?,
        Seek::End => last,
        Seek::Fraction(share) => {
            let offset = fraction_of(file.size(), share);
            text::line_start(file, detected, offset)
                .await
                .map_err(|e| e.code())?
                .min(last)
        }
    };
    let page = text::read_lines(file, detected, top, rows)
        .await
        .map_err(|e| e.code())?;
    let lines: Vec<String> = page
        .lines
        .iter()
        .map(|line| shown(&line.text, layout))
        .collect();
    let mut span = None;
    if let Some(found) = mark {
        let starts: Vec<u64> = page.lines.iter().map(|line| line.offset).collect();
        if let Some(index) = starts.iter().rposition(|&start| start <= found.offset)
            && found.offset < page.end
        {
            let start = starts[index];
            let line_end = starts.get(index + 1).copied().unwrap_or(page.end);
            let match_end = (found.offset + found.len).min(line_end);
            let bytes = file
                .read_at(start, to_len(match_end.saturating_sub(start)))
                .await
                .map_err(|e| e.code())?;
            let cut = to_len(found.offset - start).min(bytes.len());
            let before = shown(&encoding::decode(&bytes[..cut], detected.encoding), layout);
            let through = shown(&encoding::decode(&bytes, detected.encoding), layout);
            let (from, to) = mark_columns(&lines[index], &before, &through);
            span = Some(Span {
                from: (index, from),
                to: (index, to),
            });
        }
    }
    Ok(Page {
        top,
        end: page.end,
        lines,
        mark: span,
        raw: page.lines,
    })
}

async fn rows_page(
    file: &FileWindow,
    layout: Layout,
    top: u64,
    seek: Seek,
    mark: Option<Match>,
) -> Result<Page, &'static str> {
    let columns = row_columns(layout.mode);
    let step = columns as u64;
    let rows = layout.rows.max(1);
    let size = file.size();
    let last =
        hex::row_start(size.saturating_sub(1), columns).saturating_sub(step * (rows as u64 - 1));
    let top = match seek {
        Seek::Stay => top,
        Seek::At(offset) => hex::row_start(offset, columns),
        Seek::Down(_) if top >= last => top,
        Seek::Down(count) => (top + step * count as u64).min(last),
        Seek::Up(count) => top.saturating_sub(step * count as u64),
        Seek::End => last,
        Seek::Fraction(share) => hex::row_start(fraction_of(size, share), columns).min(last),
    };
    let read = hex::read_rows(file, top, rows, columns)
        .await
        .map_err(|e| e.code())?;
    let lines: Vec<String> = read
        .iter()
        .map(|row| match layout.mode {
            ViewMode::Hex => row.to_xxd(columns),
            _ => binary_line(&row.bytes, layout.detected.encoding),
        })
        .collect();
    let end = read
        .last()
        .map_or(top, |row| row.offset + row.bytes.len() as u64);
    let span = mark
        .filter(|found| found.offset >= top && found.offset < end && found.len > 0)
        .map(|found| {
            let last_byte = (found.offset + found.len - 1).min(end - 1);
            let place = |offset: u64, after: bool| {
                let line = to_len((offset - top) / step);
                let index = to_len((offset - top) % step);
                let column = match layout.mode {
                    // After `offset: ` (eight digits, more past 4 GiB), two digits a byte and
                    // a space before every pair.
                    ViewMode::Hex => {
                        let bytes_at = lines[line].find(": ").map_or(10, |colon| colon + 2);
                        bytes_at + index * 2 + index / 2 + if after { 2 } else { 0 }
                    }
                    _ => char_column(&lines[line], index + usize::from(after)),
                };
                (line, column)
            };
            Span {
                from: place(found.offset, false),
                to: place(last_byte, true),
            }
        });
    Ok(Page {
        top,
        end,
        lines,
        mark: span,
        raw: Vec::new(),
    })
}

/// The byte columns of `line` a match spans, from what is `before` it and what runs `through`
/// its end, both decoded as the line is. A match that starts inside a character - a hex
/// search, a text in UTF-16 at an odd offset - takes that whole character: where the cut split
/// it, the two decodings part (RC-080-44). The columns lie on characters of `line`.
fn mark_columns(line: &str, before: &str, through: &str) -> (usize, usize) {
    let whole = before
        .chars()
        .zip(through.chars())
        .take_while(|(cut, full)| cut == full)
        .count();
    let column = |chars: usize| {
        line.char_indices()
            .nth(chars)
            .map_or(line.len(), |(at, _)| at)
    };
    (column(whole), column(through.chars().count()))
}

/// The decoder of a picture stopped without an answer (it panicked): logged, and the picture is
/// broken.
fn decoder_gone(error: &tokio::task::JoinError) -> &'static str {
    tracing::warn!(%error, "the picture decoder stopped");
    "image_broken"
}

/// A whole picture, decoded on a blocking thread and scaled down to the pixels the view shows
/// (`viewer.picture_megapixels`); its own size is the picture's.
pub async fn picture(
    file: Arc<FileWindow>,
    format: ImageFormat,
    db: Option<Db>,
) -> Result<Picture, &'static str> {
    if file.size() > picture::MAX_FILE_BYTES {
        return Err("image_too_large");
    }
    let shown = shown_megapixels(db).await * 1_000_000;
    let bytes = file
        .read_uncached(0, to_len(file.size()))
        .await
        .map_err(|e| e.code())?;
    let pixels = tokio::task::spawn_blocking(move || picture::decode(&bytes, format, shown))
        .await
        .map_err(|error| decoder_gone(&error))??;
    Ok(ready(pixels, format.label()))
}

/// A raw RGBA raster of `width` by `height` a viewer plugin wrote (RC-100-29): exactly
/// `width * height * 4` bytes, within the picture limits, scaled down as a file's picture is.
pub async fn raster(
    file: Arc<FileWindow>,
    width: u32,
    height: u32,
    db: Option<Db>,
) -> Result<Picture, &'static str> {
    picture::check_size(width, height)?;
    let length = u64::from(width) * u64::from(height) * 4;
    if file.size() != length {
        return Err("image_broken");
    }
    if length > picture::MAX_FILE_BYTES {
        return Err("image_too_large");
    }
    let shown_pixels = shown_megapixels(db).await * 1_000_000;
    let bytes = file
        .read_uncached(0, to_len(length))
        .await
        .map_err(|e| e.code())?;
    let pixels =
        tokio::task::spawn_blocking(move || picture::raster(bytes, width, height, shown_pixels))
            .await
            .map_err(|error| decoder_gone(&error))??;
    Ok(ready(pixels, "RGBA"))
}

/// The image widget's picture of `pixels`.
fn ready(pixels: picture::Pixels, format: &'static str) -> Picture {
    Picture {
        width: pixels.width,
        height: pixels.height,
        handle: Handle::from_rgba(pixels.shown_width, pixels.shown_height, pixels.rgba),
        format,
    }
}

/// `viewer.picture_megapixels`; its default without a database or a valid value.
async fn shown_megapixels(db: Option<Db>) -> u64 {
    let Some(db) = db else {
        return picture::DEFAULT_SHOWN_MEGAPIXELS;
    };
    stored_setting(&db, PICTURE_MEGAPIXELS_KEY)
        .await
        .and_then(|value| value.as_u64())
        .filter(|megapixels| *megapixels > 0)
        .unwrap_or(picture::DEFAULT_SHOWN_MEGAPIXELS)
}

/// A search through the whole file, its progress and end as a stream; `cancel` stops it.
pub fn search(
    file: Arc<FileWindow>,
    needle: Needle,
    from: u64,
    direction: Direction,
    cancel: Arc<AtomicBool>,
) -> impl Stream<Item = Found> {
    iced::stream::channel(8, async move |mut sender: mpsc::Sender<Found>| {
        let mut ticks = sender.clone();
        let mut progress = move |bytes: u64| {
            // A full channel drops a tick; the next one says more.
            let _ = ticks.try_send(Found::Progress(bytes));
        };
        let outcome = viewer::search(&file, &needle, from, direction, &cancel, &mut progress)
            .await
            .map_err(|e| e.code());
        // The receiver is gone only when the viewer closed: nobody waits for the end.
        let _ = sender.send(Found::Done(outcome)).await;
    })
}

/// A line as the text view shows it: tabs expanded, control characters as dots, the first
/// `layout.left` characters cut off.
pub fn shown(line: &str, layout: Layout) -> String {
    text::expand_tabs(line, layout.tab_width)
        .chars()
        .skip(layout.left)
        .map(|c| if c.is_control() { '.' } else { c })
        .collect()
}

/// Bytes as the binary view shows them: each one a character of a single-byte encoding (the
/// chosen one, Windows-1252 for a multi-byte one), control characters as dots.
pub fn binary_line(bytes: &[u8], encoding: TextEncoding) -> String {
    let single = match encoding {
        TextEncoding::Latin1 | TextEncoding::Cp437 => encoding,
        _ => TextEncoding::Windows1252,
    };
    bytes
        .iter()
        .map(|&byte| {
            encoding::decode(&[byte], single)
                .chars()
                .next()
                .filter(|c| !c.is_control())
                .unwrap_or('.')
        })
        .collect()
}

/// The bytes per row of a row view.
pub fn row_columns(mode: ViewMode) -> usize {
    match mode {
        ViewMode::Binary => BINARY_COLUMNS,
        _ => hex::BYTES_PER_ROW,
    }
}

/// The byte column of the `index`-th character of `line`, or its end.
fn char_column(line: &str, index: usize) -> usize {
    line.char_indices()
        .nth(index)
        .map_or(line.len(), |(at, _)| at)
}

fn fraction_of(size: u64, share: f32) -> u64 {
    let share = f64::from(share.clamp(0.0, 1.0));
    // A share of a size is a size: the cast cannot leave the file.
    (size as f64 * share) as u64
}

fn to_len(bytes: u64) -> usize {
    usize::try_from(bytes).unwrap_or(usize::MAX)
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;
