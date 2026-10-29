//! The fonts the user chooses (RC-010-17, after Total Commander's Configuration, Fonts): a family
//! and a size for the file lists (both panels, the tabs, the path line), the terminal and the
//! viewer. The rest of the window stays in the interface font ([`super::UI_FONT`]).
//!
//! The settings are read off the UI thread, resolved against the font database iced loads anyway
//! ([`installed`]) - a family this machine does not have falls back to the area's default - and
//! kept here for the views, as the stripes are ([`set_fonts`], [`fonts`]). A list row's height,
//! a viewer line's and the fixed columns' widths follow the size; the middle ellipsis measures in
//! the chosen font's kind.
//!
//! `Font::with_name` takes a `&'static str`: [`intern`] keeps each chosen name once, at most
//! [`MAX_NAMES`] of them - never the whole list of installed fonts.

use std::{
    borrow::Cow,
    collections::BTreeMap,
    ops::RangeInclusive,
    sync::{Mutex, PoisonError, RwLock},
    time::Duration,
};

use iced::{
    Font,
    advanced::graphics::text::{
        cosmic_text::{
            fontdb,
            skrifa::{
                FontRef, MetadataProvider,
                instance::{LocationRef, Size},
            },
        },
        font_system,
    },
    font::Family as IcedFamily,
};
use rc_engine::settings::{MAX_FONT_SIZE, MIN_FONT_SIZE, SettingKind};

use super::{MONO_FONT, TEXT_SIZE, UI_FONT, ellipsis};

/// The most font names kept for `Font::with_name`; a name past them draws in the default.
pub const MAX_NAMES: usize = 64;
/// The families of iced's own icons and the buttons' ([`super::icon`]), no fonts to read text in.
const ICON_FAMILIES: [&str; 2] = ["Iced-Icons", super::icon::FAMILY];

/// The font of one area: its iced font, its size in points and whether it is monospaced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AreaFont {
    pub font: Font,
    pub size: f32,
    pub mono: bool,
}

impl AreaFont {
    /// A file list row's height: the text's line and room around it; 24 at the default 13.
    pub fn row_height(self) -> f32 {
        (self.size * 1.3 + 7.0).round()
    }

    /// A viewer line's height; 18 at the default 13.
    pub fn line_height(self) -> f32 {
        (self.size * 1.4).round()
    }

    /// How much wider than at the default the fixed columns (extension, size, date, rights) are:
    /// with the size, and a fifth more in a monospaced font, whose digits are wider.
    pub fn column_scale(self) -> f32 {
        let mono = if self.mono { 1.2 } else { 1.0 };
        self.size / TEXT_SIZE * mono
    }

    /// The estimated width of `text` in this font.
    pub fn width(self, text: &str) -> f32 {
        if self.mono {
            ellipsis::estimate_mono_width(text, self.size)
        } else {
            ellipsis::estimate_width(text, self.size)
        }
    }

    /// `text` shortened in the middle to `max_width` in this font.
    pub fn shorten(self, text: &str, max_width: f32) -> Cow<'_, str> {
        let measure = |part: &str| self.width(part);
        ellipsis::middle_ellipsis_by(text, max_width, measure)
    }

    /// `text` shortened at its end to `max_width` in this font.
    pub fn shorten_end(self, text: &str, max_width: f32) -> Cow<'_, str> {
        let measure = |part: &str| self.width(part);
        ellipsis::end_ellipsis_by(text, max_width, measure)
    }
}

/// How a file list shows a name too long for its column (owner, 2026-10-19):
/// `appearance.name_cut` and `appearance.name_tip_delay`; and whether a hidden or system
/// entry's icon carries a `!` (`appearance.mark_hidden`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Names {
    /// Cut at the end, as the Explorer does; else in the middle.
    pub cut_end: bool,
    /// How long the pointer rests on a shortened name before its tooltip shows.
    pub tip_delay: Duration,
    /// A hidden or system entry's icon is faded with a `!` over it, as in Total Commander.
    pub mark_hidden: bool,
}

impl Names {
    pub const DEFAULT: Self = Self {
        cut_end: true,
        tip_delay: Duration::from_millis(500),
        mark_hidden: true,
    };
}

/// The fonts of the three areas, and how the lists shorten a name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fonts {
    pub list: AreaFont,
    pub terminal: AreaFont,
    pub viewer: AreaFont,
    pub names: Names,
    /// How tall the viewer font's full block draws, as a share of its size, while
    /// `viewer.dos_tight_lines` is on: DOS text draws its lines that tall ([`block_height`]).
    pub dos_block: Option<f32>,
}

impl Fonts {
    /// As before the fonts could be chosen: the interface font in the lists, Cascadia Mono in
    /// the terminal and the viewer, all at 13.
    pub const DEFAULT: Self = Self {
        list: AreaFont {
            font: UI_FONT,
            size: TEXT_SIZE,
            mono: false,
        },
        terminal: AreaFont {
            font: MONO_FONT,
            size: TEXT_SIZE,
            mono: true,
        },
        viewer: AreaFont {
            font: MONO_FONT,
            size: TEXT_SIZE,
            mono: true,
        },
        names: Names::DEFAULT,
        dos_block: None,
    };
}

impl Default for Fonts {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// The fonts in force, read at the start and on every change; the defaults until then.
static FONTS: RwLock<Fonts> = RwLock::new(Fonts::DEFAULT);

/// Puts `fonts` in force; the next frame draws with them.
pub fn set_fonts(fonts: Fonts) {
    *FONTS.write().unwrap_or_else(PoisonError::into_inner) = fonts;
}

/// The fonts in force.
pub fn fonts() -> Fonts {
    *FONTS.read().unwrap_or_else(PoisonError::into_inner)
}

/// The file lists' font in force.
pub fn list() -> AreaFont {
    fonts().list
}

/// The chosen family `name` at `size` points: `default` in that size when the name is empty, the
/// family is not installed (`installed` answers whether it is monospaced, `None` when it is not
/// there) or no more names can be kept. The size is held to the area's range in the schema
/// (`sizes`): 8 to 24, the viewer's 6 to 72.
pub fn resolve(
    name: &str,
    size: i64,
    sizes: RangeInclusive<i64>,
    default: AreaFont,
    installed: impl Fn(&str) -> Option<bool>,
) -> AreaFont {
    let size = size.clamp(*sizes.start(), *sizes.end()) as f32;
    let fallback = AreaFont { size, ..default };
    if name.is_empty() {
        return fallback;
    }
    // The settings dialog says when a chosen family is not installed.
    let Some(mono) = installed(name) else {
        return fallback;
    };
    match intern(name) {
        Some(name) => AreaFont {
            font: Font::with_name(name),
            size,
            mono,
        },
        None => fallback,
    }
}

/// How tall a full block (U+2588) draws in `font`, as a share of its size: the line height at
/// which the blocks of DOS art touch, whatever ascent and descent the font declares. `None` when
/// the family is not installed, has no such glyph or an implausible one. It locks the font
/// database only to find the face, and reads the font's data after (RC-080-53): the window draws
/// meanwhile. Called off the UI thread, when the settings are read.
pub fn block_height(font: Font) -> Option<f32> {
    let family = match font.family {
        IcedFamily::Name(name) => fontdb::Family::Name(name),
        IcedFamily::Monospace => fontdb::Family::Monospace,
        _ => return None,
    };
    let face = {
        let mut system = font_system()
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        let db = system.raw().db();
        let id = db.query(&fontdb::Query {
            families: &[family],
            ..fontdb::Query::default()
        })?;
        db.face(id)?.clone()
    };
    // A database of the one face, to read its data without the shared one's lock.
    let mut one = fontdb::Database::new();
    let id = one.push_face_info(face);
    one.with_face_data(id, |data, index| {
        let face = FontRef::from_index(data, index).ok()?;
        let glyph = face.charmap().map('\u{2588}')?;
        // At a size of one pixel per em the bounds are the share of the size.
        let bounds = face
            .glyph_metrics(Size::new(1.0), LocationRef::default())
            .bounds(glyph)?;
        plausible_block(bounds.y_max - bounds.y_min)
    })?
}

/// A block's height as a share of the font size, kept when a line could be that tall.
fn plausible_block(height: f32) -> Option<f32> {
    (0.8..=2.0).contains(&height).then_some(height)
}

/// The sizes the schema allows for the size setting `key`: 8 to 24, the viewer's 6 to 72
/// (RC-020-12); 8 to 24 for a key that is no size.
pub fn sizes_of(key: &str) -> RangeInclusive<i64> {
    match rc_engine::settings::setting(key).map(|spec| spec.kind) {
        Some(SettingKind::Int { min, max, .. }) => min..=max,
        _ => MIN_FONT_SIZE..=MAX_FONT_SIZE,
    }
}

static NAMES: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

/// `name` for `Font::with_name`: kept once for the program's life, at most [`MAX_NAMES`] names;
/// `None` past them.
pub fn intern(name: &str) -> Option<&'static str> {
    let mut names = NAMES.lock().unwrap_or_else(PoisonError::into_inner);
    intern_in(&mut names, name, MAX_NAMES)
}

fn intern_in(names: &mut Vec<&'static str>, name: &str, limit: usize) -> Option<&'static str> {
    if let Some(kept) = names.iter().copied().find(|kept| *kept == name) {
        return Some(kept);
    }
    if names.len() >= limit {
        return None;
    }
    let kept: &'static str = Box::leak(Box::<str>::from(name));
    names.push(kept);
    Some(kept)
}

/// An installed font family.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Family {
    pub name: String,
    pub mono: bool,
}

/// The font families of iced's font database (the system's, loaded once), by name, each once.
/// It locks the database: called off the UI thread, when the settings are read.
pub fn installed() -> Vec<Family> {
    // The names under the lock, the sorting after it (RC-080-53).
    let faces: Vec<(String, bool)> = {
        let mut system = font_system()
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        system
            .raw()
            .db()
            .faces()
            .filter_map(|face| {
                let (name, _) = face.families.first()?;
                Some((name.clone(), face.monospaced))
            })
            .collect()
    };
    families(faces.into_iter())
}

/// The families of `faces` (a family's name, whether the face is monospaced): sorted, each once,
/// monospaced when one of its faces is; hidden (`.`) families and the icon fonts left out.
fn families(faces: impl Iterator<Item = (String, bool)>) -> Vec<Family> {
    let mut seen: BTreeMap<String, bool> = BTreeMap::new();
    for (name, mono) in faces {
        if name.is_empty() || name.starts_with('.') || ICON_FAMILIES.contains(&name.as_str()) {
            continue;
        }
        *seen.entry(name).or_default() |= mono;
    }
    let mut families: Vec<Family> = seen
        .into_iter()
        .map(|(name, mono)| Family { name, mono })
        .collect();
    families.sort_by_cached_key(|family| family.name.to_lowercase());
    families
}

#[cfg(test)]
#[path = "fonts_tests.rs"]
mod tests;
