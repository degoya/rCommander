//! The tags of an ISO base media file: the iTunes list (`udta` > `meta` > `ilst`), QuickTime's
//! keyed list (`moov` > `meta` with `keys`, as a phone writes make, model and location) and
//! QuickTime's own text items in `udta` (`\xa9nam`, `\xa9xyz`). Values are read up to [`MAX_TEXT`]
//! bytes; a cover (`covr`) up to [`MAX_COVER`], and only when it is to be shown.

use crate::bmff::{BoxHeader, Boxes};
use crate::io::{be_u16, be_u32, be_uint};
use crate::outline::Node;
use crate::types::{Failure, Source};
use crate::{Artwork, ImageFormat, codes, units};

/// The largest cover passed on as the `image` output.
pub const MAX_COVER: u64 = 8 << 20;
/// The most bytes of one value read.
const MAX_TEXT: usize = 4096;
/// The most items listed.
const MAX_ITEMS: usize = 1000;

/// What the tags of one file said.
#[derive(Default)]
pub struct Items {
    pub title: Option<String>,
    pub facts: Vec<Node>,
    pub artwork: Option<Artwork>,
}

/// `udta`: an iTunes `meta`, and QuickTime's text items.
pub fn read_udta<S: Source>(
    boxes: &mut Boxes<'_, '_, S>,
    udta: &BoxHeader,
    want_cover: bool,
    items: &mut Items,
) -> Result<(), Failure> {
    for child in boxes.children(udta.body, udta.end)? {
        if items.facts.len() >= MAX_ITEMS {
            break;
        }
        if &child.kind == b"meta" {
            read_meta(boxes, &child, want_cover, items)?;
        } else if child.kind[0] == 0xA9 {
            // A QuickTime text item: its size, its language, the text.
            let head = boxes.head(&child, 4 + MAX_TEXT)?;
            let length = usize::from(be_u16(&head, 0).unwrap_or(0));
            let text = head.get(4..).unwrap_or_default();
            let text = String::from_utf8_lossy(text.get(..length).unwrap_or(text)).into_owned();
            push(items, item_label(&child.kind), text);
        }
    }
    Ok(())
}

/// `meta`: its `keys`, if any, and its `ilst`. A full box in ISO files, a plain one in QuickTime:
/// the children start after the version and flags unless a known box starts right away.
pub fn read_meta<S: Source>(
    boxes: &mut Boxes<'_, '_, S>,
    meta: &BoxHeader,
    want_cover: bool,
    items: &mut Items,
) -> Result<(), Failure> {
    let peek = boxes.head(meta, 8)?;
    let plain = matches!(peek.get(4..8), Some(b"hdlr" | b"ilst" | b"keys" | b"mhdr"));
    let start = if plain { meta.body } else { meta.body + 4 };
    if start >= meta.end {
        return Ok(());
    }
    let children = boxes.children(start, meta.end)?;
    let keys = match children.iter().find(|child| &child.kind == b"keys") {
        Some(keys) => read_keys(boxes, keys)?,
        None => Vec::new(),
    };
    if let Some(ilst) = children.iter().find(|child| &child.kind == b"ilst") {
        read_ilst(boxes, ilst, &keys, want_cover, items)?;
    }
    Ok(())
}

/// `keys`: version and flags, the count, then per key a box of its namespace whose body is the
/// key's name.
fn read_keys<S: Source>(
    boxes: &mut Boxes<'_, '_, S>,
    keys: &BoxHeader,
) -> Result<Vec<String>, Failure> {
    let start = keys.body.saturating_add(8).min(keys.end);
    let mut names = Vec::new();
    for key in boxes.children(start, keys.end)? {
        let name = boxes.head(&key, MAX_TEXT)?;
        names.push(String::from_utf8_lossy(&name).into_owned());
    }
    Ok(names)
}

fn read_ilst<S: Source>(
    boxes: &mut Boxes<'_, '_, S>,
    ilst: &BoxHeader,
    keys: &[String],
    want_cover: bool,
    items: &mut Items,
) -> Result<(), Failure> {
    for item in boxes.children(ilst.body, ilst.end)? {
        if items.facts.len() >= MAX_ITEMS {
            break;
        }
        let children = boxes.children(item.body, item.end)?;
        let Some(data) = children.iter().find(|child| &child.kind == b"data") else {
            continue;
        };
        let label = if &item.kind == b"----" {
            match children.iter().find(|child| &child.kind == b"name") {
                Some(name) => {
                    let name = boxes.head(name, 4 + MAX_TEXT)?;
                    String::from_utf8_lossy(name.get(4..).unwrap_or_default()).into_owned()
                }
                None => "----".to_owned(),
            }
        } else if let Some(key) = keyed(&item.kind, keys) {
            key_label(key)
        } else {
            item_label(&item.kind)
        };
        // The data box: its type (the low 24 bits), its locale, then the value.
        let data_type = be_u32(&boxes.head(data, 4)?, 0).map_or(0, |value| value & 0x00FF_FFFF);
        let start = data.body.saturating_add(8);
        if start > data.end {
            return Err(Failure::bad(codes::BAD_VIDEO));
        }
        let length = data.end - start;
        if &item.kind == b"covr" {
            if want_cover && items.artwork.is_none() && length <= MAX_COVER {
                let bytes = boxes.reader.exact(start, length, codes::BAD_VIDEO)?;
                if let Some(format) = ImageFormat::sniff(&bytes) {
                    items.artwork = Some(Artwork { format, bytes });
                }
            }
            items
                .facts
                .push(Node::fact("Cover", format!("{length} bytes")));
            continue;
        }
        let bytes = boxes.reader.head(
            start,
            usize::try_from(length).unwrap_or(MAX_TEXT).min(MAX_TEXT),
        )?;
        let value = match &item.kind {
            b"trkn" | b"disk" => pair(&bytes),
            _ => value_of(data_type, &bytes, length),
        };
        push(items, label, value);
    }
    Ok(())
}

/// Adds a fact, and the title when it is one.
fn push(items: &mut Items, label: String, value: String) {
    if items.title.is_none() && label == "Title" && !value.trim().is_empty() {
        items.title = Some(value.clone());
    }
    items.facts.push(Node::fact(label, value));
}

/// The key a QuickTime item (named by its 1-based index into `keys`) stands for.
fn keyed<'k>(kind: &[u8; 4], keys: &'k [String]) -> Option<&'k String> {
    let index = usize::try_from(u32::from_be_bytes(*kind)).ok()?;
    keys.get(index.checked_sub(1)?)
}

/// A QuickTime key for a person: `com.apple.quicktime.location.ISO6709` as `Location`.
fn key_label(key: &str) -> String {
    let name = key.strip_prefix("com.apple.quicktime.").unwrap_or(key);
    match name {
        "title" | "displayname" => "Title".to_owned(),
        "artist" | "author" => "Artist".to_owned(),
        "album" => "Album".to_owned(),
        "comment" => "Comment".to_owned(),
        "description" => "Description".to_owned(),
        "make" => "Make".to_owned(),
        "model" => "Model".to_owned(),
        "software" => "Software".to_owned(),
        "creationdate" => "Creation date".to_owned(),
        "location.ISO6709" => "Location".to_owned(),
        other => other.to_owned(),
    }
}

/// An iTunes or QuickTime item for a person, by its type.
fn item_label(kind: &[u8; 4]) -> String {
    let label = match kind {
        b"\xa9nam" => "Title",
        b"\xa9ART" | b"\xa9aut" => "Artist",
        b"aART" => "Album artist",
        b"\xa9alb" => "Album",
        b"\xa9day" => "Date",
        b"\xa9gen" | b"gnre" => "Genre",
        b"\xa9cmt" => "Comment",
        b"\xa9wrt" => "Composer",
        b"\xa9too" | b"\xa9enc" => "Encoder",
        b"\xa9des" | b"desc" => "Description",
        b"ldes" => "Long description",
        b"\xa9cpy" | b"cprt" => "Copyright",
        b"\xa9lyr" => "Lyrics",
        b"\xa9xyz" => "Location",
        b"\xa9mak" => "Make",
        b"\xa9mod" => "Model",
        b"\xa9swr" => "Software",
        b"trkn" => "Track",
        b"disk" => "Disc",
        b"tvsh" => "Show",
        b"tvsn" => "Season",
        b"tves" => "Episode",
        _ => return units::fourcc(kind),
    };
    label.to_owned()
}

/// A track or disc number: two reserved bytes, the number and the count.
fn pair(bytes: &[u8]) -> String {
    match (be_u16(bytes, 2), be_u16(bytes, 4)) {
        (Some(number), Some(count)) if count > 0 => format!("{number}/{count}"),
        (Some(number), _) => number.to_string(),
        _ => format!("{} bytes", bytes.len()),
    }
}

/// A data box's value by its type: 1 UTF-8, 2 UTF-16, 21 and 22 signed and unsigned integers;
/// anything else as text when it is printable text, else by its size.
fn value_of(data_type: u32, bytes: &[u8], length: u64) -> String {
    match data_type {
        1 => String::from_utf8_lossy(bytes).into_owned(),
        2 => {
            let code_units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_be_bytes(*pair))
                .collect();
            String::from_utf16_lossy(&code_units)
        }
        21 if !bytes.is_empty() && bytes.len() <= 8 => {
            let shift = 64 - 8 * bytes.len() as u32;
            #[allow(
                clippy::cast_possible_wrap,
                reason = "the bytes are a two's complement number, read whole and sign-extended"
            )]
            let value = ((be_uint(bytes) << shift) as i64) >> shift;
            value.to_string()
        }
        22 if !bytes.is_empty() && bytes.len() <= 8 => be_uint(bytes).to_string(),
        _ => match std::str::from_utf8(bytes) {
            Ok(text) if !text.chars().any(|c| c.is_control() && c != '\n') => text.to_owned(),
            _ => format!("{length} bytes"),
        },
    }
}

#[cfg(test)]
#[path = "bmff_items_tests.rs"]
mod tests;
