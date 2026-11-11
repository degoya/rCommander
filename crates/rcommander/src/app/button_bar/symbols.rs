//! The buttons' symbols (RC-100-87), read off the window's thread and kept by file, index and
//! size. A `.png`, `.ico`, `.bmp`, `.gif`, `.jpg` or `.webp` is decoded everywhere, as the
//! viewer decodes a picture: at most [`MAX_FILE_BYTES`] of it, at most [`MAX_SIDE`] pixels a
//! side, scaled to the bars' size. On Windows an `.exe`, a `.dll` or an `.ico` gives the icon at
//! its index at the bars' size, scaled from its next larger image (`SHDefExtractIconW`,
//! `app/icons`, RC-110-15), any other file its own icon as Explorer shows it -
//! also a program's button without a symbol of its own, by its program. An own command's button
//! without one shows the own command's symbol, read again whenever the own commands are. A
//! symbol that does not read is asked for once a run; its button shows its text. A symbol of the
//! collection (`rc-symbol:<name>`, RC-110-10) is no file: `glyphs.rs` draws it.

use std::{
    collections::HashMap,
    io::{Cursor, Read as _},
    path::Path,
};

use ::image::{ImageFormat, ImageReader, Limits};
use iced::{Task, widget::image::Handle};
use rc_engine::{BarButton, ButtonAction, CommandKind, UserCommand};

use super::{BbMsg, glyphs, wrap};
use crate::{
    app::{Message, icons::Rgba, user_commands::known},
    recover::LogPanic as _,
};

/// The largest symbol file read, in bytes.
pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
/// The longest side of a picture decoded.
pub const MAX_SIDE: u32 = 1024;

/// A symbol: its file, the icon's index in it, the size it is shown at.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SymbolKey {
    pub path: String,
    #[cfg_attr(
        not(windows),
        allow(
            dead_code,
            reason = "an icon's index picks one of a program's on Windows only"
        )
    )]
    pub index: i64,
    pub size: u16,
}

/// The symbols read, and those asked for.
#[derive(Debug, Default)]
pub struct Symbols {
    /// Each symbol asked for: its image once read, `None` while it is read or when it did not.
    known: HashMap<SymbolKey, Option<Handle>>,
}

impl Symbols {
    /// The image of `key`, once read.
    pub fn get(&self, key: &SymbolKey) -> Option<&Handle> {
        self.known.get(key).and_then(Option::as_ref)
    }

    /// The task that reads those of `wanted` not asked for before; `None` when there are none.
    pub fn ask(&mut self, wanted: Vec<SymbolKey>) -> Option<Task<Message>> {
        let mut new: Vec<SymbolKey> = Vec::new();
        for key in wanted {
            if !self.known.contains_key(&key) && !new.contains(&key) {
                new.push(key);
            }
        }
        if new.is_empty() {
            return None;
        }
        for key in &new {
            self.known.insert(key.clone(), None);
        }
        Some(Task::perform(
            async move {
                tokio::task::spawn_blocking(move || {
                    new.into_iter()
                        .map(|key| {
                            let read = read(&key);
                            (key, read)
                        })
                        .collect::<Vec<_>>()
                })
                .await
                .log_panic("button_bar.symbols")
                .unwrap_or_default()
            },
            |read| wrap(BbMsg::Symbols(read)),
        ))
    }

    /// The symbols read: each kept as an image, one that did not read as none.
    pub fn loaded(&mut self, read: Vec<(SymbolKey, Option<Rgba>)>) {
        for (key, pixels) in read {
            let image = pixels.map(|rgba| Handle::from_rgba(rgba.width, rgba.height, rgba.pixels));
            self.known.insert(key, image);
        }
    }
}

/// The symbol `button` shows at `size`: its own; an own command's button that has none the own
/// command's (its `icon_path` and `icon_index`); or on Windows a program's by its absolute path.
/// `None` for a button that shows a glyph or its text.
pub fn key_of(button: &BarButton, size: u16) -> Option<SymbolKey> {
    // The own commands are copied only for a button that needs them.
    let own = button.icon.is_none()
        && matches!(&button.action, ButtonAction::Command(action) if action.kind == CommandKind::Own);
    let commands = if own { known::commands() } else { Vec::new() };
    key_among(button, size, &commands)
}

/// [`key_of`] with the own commands `commands`.
pub fn key_among(button: &BarButton, size: u16, commands: &[UserCommand]) -> Option<SymbolKey> {
    if let Some(icon) = &button.icon {
        // A symbol of the collection is a glyph (`glyphs::collection`), no file to read.
        if glyphs::is_collection(&icon.path) {
            return None;
        }
        return Some(SymbolKey {
            path: icon.path.clone(),
            index: icon.index,
            size,
        });
    }
    match &button.action {
        ButtonAction::Command(action) if action.kind == CommandKind::Own => commands
            .iter()
            .find(|command| command.name.eq_ignore_ascii_case(action.command.trim()))
            .filter(|command| {
                !command.icon_path.trim().is_empty() && !glyphs::is_collection(&command.icon_path)
            })
            .map(|command| SymbolKey {
                path: command.icon_path.trim().to_owned(),
                index: command.icon_index,
                size,
            }),
        ButtonAction::Command(action)
            if cfg!(windows)
                && action.kind == CommandKind::Program
                && Path::new(&action.command).is_absolute() =>
        {
            Some(SymbolKey {
                path: action.command.clone(),
                index: 0,
                size,
            })
        }
        _ => None,
    }
}

/// The picture format a symbol file's extension names; `None` for any other file.
fn picture_format(path: &Path) -> Option<ImageFormat> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some(ImageFormat::Png),
        "ico" => Some(ImageFormat::Ico),
        "bmp" => Some(ImageFormat::Bmp),
        "gif" => Some(ImageFormat::Gif),
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

/// The pixels of `key`; blocks.
fn read(key: &SymbolKey) -> Option<Rgba> {
    let path = Path::new(&key.path);
    if !path.is_absolute() {
        return None;
    }
    #[cfg(windows)]
    {
        // A program, a library or an icon by its index; any other file its own icon.
        let windows_icon = picture_format(path).is_none_or(|format| format == ImageFormat::Ico);
        if windows_icon && let Some(pixels) = crate::app::icons::symbol(path, key.index, key.size) {
            return Some(pixels);
        }
    }
    let format = picture_format(path)?;
    let bytes = read_bounded(path)?;
    decode(&bytes, format, u32::from(key.size))
}

/// At most [`MAX_FILE_BYTES`] of the regular file at `path`; `None` for more, or none.
fn read_bounded(path: &Path) -> Option<Vec<u8>> {
    let file = std::fs::File::open(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).ok()?;
    (u64::try_from(bytes.len()).ok()? <= MAX_FILE_BYTES).then_some(bytes)
}

/// `bytes` in `format`, decoded within the limits and scaled to fit `size` pixels.
pub fn decode(bytes: &[u8], format: ImageFormat, size: u32) -> Option<Rgba> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(u64::from(MAX_SIDE) * u64::from(MAX_SIDE) * 4);
    reader.limits(limits);
    let mut picture = reader.decode().ok()?;
    if picture.width() > size || picture.height() > size {
        picture = picture.thumbnail(size, size);
    }
    let rgba = picture.into_rgba8();
    Some(Rgba {
        width: rgba.width(),
        height: rgba.height(),
        pixels: rgba.into_raw(),
    })
}

#[cfg(test)]
#[path = "symbols_tests.rs"]
mod tests;
