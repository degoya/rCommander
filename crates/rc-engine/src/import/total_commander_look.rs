//! The look of `wincmd.ini` read into plain values (RC-050-14): the colours of `[Colors]` for the
//! light look and of `[ColorsDark]` for the dark one, the rules by file type (`ColorFilterN`), and
//! the file lists' font of `[AllResolutions]`. Nothing here touches the database;
//! `total_commander_look_apply` compares and writes.
//!
//! - A colour is Total Commander's `COLORREF` in decimal, `0x00BBGGRR`. A negative one is its
//!   default (or a system colour) and gives no row: rCommander's own stays.
//! - `ThemedCursor` other than `0` takes the theme's cursor: no row for the cursor's colours.
//!   `InverseCursor` and `InverseSelection` other than `0` have no counterpart: a skipped row each.
//! - A rule's dark colour is `ColorFilterNColor` of `[ColorsDark]`; without one the light colour
//!   holds for both. A mask that starts with `>` names a saved search, which rCommander has not.
//! - The font size is in points, rCommander's in logical pixels (its 13 is Total Commander's 10 at
//!   96 dpi): times 4/3, rounded, within [`MIN_FONT_SIZE`] and [`MAX_FONT_SIZE`]. The sections per
//!   screen resolution are left alone: they belong to one screen.

use serde_json::json;

use super::{
    tc_ini::Ini,
    total_commander::{MAX_ITEMS, TcSkip, flag},
};
use crate::settings::{
    ColourRule, LIST_FONT_KEY, MAX_FONT_SIZE, MIN_FONT_SIZE, parse_colour_rules, setting,
    write_colour_rules,
};

/// What one row of the look sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TcLookKind {
    /// The colour `token` ([`crate::settings::COLOUR_TOKENS`]) of the light or the dark look.
    Colour {
        dark: bool,
        token: &'static str,
        value: String,
    },
    /// A rule by file type.
    Rule(ColourRule),
    /// The file lists' font, with its size when Total Commander gives one.
    ListFont { family: String, size: Option<i64> },
}

/// One colour, rule or font of `wincmd.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TcLook {
    /// `colour.<light|dark>.<token>`, `rule.<N>` or `font.list`: what `only` names.
    pub key: String,
    /// Where Total Commander keeps it, as `[Colors] CursorColor=128`.
    pub source: String,
    pub kind: Result<TcLookKind, TcSkip>,
}

/// Total Commander's colour keys and the tokens they become, in the colours page's order.
const COLOURS: [(&str, &str); 6] = [
    ("ForeColor", "text"),
    ("BackColor", "background"),
    ("BackColor2", "stripe"),
    ("CursorColor", "cursor"),
    ("CursorText", "cursor_text"),
    ("MarkColor", "marked"),
];

/// The switches rCommander has no counterpart for, with their keys' suffixes.
const INVERSE: [(&str, &str); 2] = [
    ("InverseCursor", "inverse_cursor"),
    ("InverseSelection", "inverse_selection"),
];

/// `raw` as `#rrggbb`; `Ok(None)` for Total Commander's default.
fn colour(raw: &str) -> Result<Option<String>, TcSkip> {
    let value: i64 = raw.trim().parse().map_err(|_| TcSkip::ColourInvalid)?;
    if value < 0 {
        return Ok(None);
    }
    if value > 0x00FF_FFFF {
        return Err(TcSkip::ColourInvalid);
    }
    let (red, green, blue) = (value & 0xFF, (value >> 8) & 0xFF, value >> 16);
    Ok(Some(format!("#{red:02x}{green:02x}{blue:02x}")))
}

/// The colours of `[Colors]` (light) and `[ColorsDark]` (dark), in [`COLOURS`] order.
#[must_use]
pub fn colours(ini: &Ini) -> Vec<TcLook> {
    let mut found = Vec::new();
    for (section, dark) in [("Colors", false), ("ColorsDark", true)] {
        let Some(values) = ini.section(section) else {
            continue;
        };
        let look = if dark { "dark" } else { "light" };
        let themed = flag(values.get("ThemedCursor"));
        for (name, token) in COLOURS {
            let Some(raw) = values.get(name) else {
                continue;
            };
            let kind = match colour(raw) {
                _ if themed && token.starts_with("cursor") => continue,
                Ok(None) => continue,
                Ok(Some(value)) => Ok(TcLookKind::Colour { dark, token, value }),
                Err(skip) => Err(skip),
            };
            found.push(TcLook {
                key: format!("colour.{look}.{token}"),
                source: format!("[{section}] {name}={}", raw.trim()),
                kind,
            });
        }
        for (name, suffix) in INVERSE {
            if flag(values.get(name)) {
                found.push(TcLook {
                    key: format!("colour.{look}.{suffix}"),
                    source: format!("[{section}] {name}=1"),
                    kind: Err(TcSkip::Inverse),
                });
            }
        }
    }
    found
}

/// The rules by file type of `[Colors]`, from `ColorFilter1` to the first gap.
#[must_use]
pub fn rules(ini: &Ini) -> Vec<TcLook> {
    let Some(light) = ini.section("Colors") else {
        return Vec::new();
    };
    let dark = ini.section("ColorsDark");
    let mut found = Vec::new();
    for index in 1..=MAX_ITEMS {
        let Some(mask) = light.get(&format!("ColorFilter{index}")) else {
            break;
        };
        let mask = mask.trim();
        let colour_name = format!("ColorFilter{index}Color");
        let dark = dark.and_then(|dark| dark.get(&colour_name));
        found.push(TcLook {
            key: format!("rule.{index}"),
            source: format!("[Colors] ColorFilter{index}={mask}"),
            kind: rule(mask, light.get(&colour_name).unwrap_or_default(), dark),
        });
    }
    found
}

fn rule(mask: &str, light: &str, dark: Option<&str>) -> Result<TcLookKind, TcSkip> {
    if mask.starts_with('>') {
        return Err(TcSkip::SearchFilter);
    }
    let rule = ColourRule {
        mask: mask.to_owned(),
        light: colour(light)?.ok_or(TcSkip::ColourInvalid)?,
        // A dark colour that does not read leaves the light one for both.
        dark: dark.and_then(|raw| colour(raw).ok().flatten()),
    };
    // The rule as the setting would take it: a mask that is empty, too long or holds a control
    // character does not.
    match parse_colour_rules(&write_colour_rules(std::slice::from_ref(&rule))) {
        Some(read) if read == [rule.clone()] => Ok(TcLookKind::Rule(rule)),
        _ => Err(TcSkip::NameInvalid),
    }
}

/// The file lists' font of `[AllResolutions]`, when it names one.
#[must_use]
pub fn font(ini: &Ini) -> Option<TcLook> {
    let values = ini.section("AllResolutions")?;
    let family = values.get("FontName")?.trim();
    if family.is_empty() {
        return None;
    }
    let points = values.get("FontSize").map(str::trim);
    let size = points
        .and_then(|points| points.parse::<i64>().ok())
        .filter(|points| (1..=1000).contains(points))
        .map(|points| ((points * 8 + 3) / 6).clamp(MIN_FONT_SIZE, MAX_FONT_SIZE));
    let fits = setting(LIST_FONT_KEY).is_some_and(|spec| spec.check(&json!(family)).is_ok());
    let kind = if fits {
        Ok(TcLookKind::ListFont {
            family: family.to_owned(),
            size,
        })
    } else {
        Err(TcSkip::NameInvalid)
    };
    let source = match points {
        Some(points) => format!("[AllResolutions] FontName={family}, FontSize={points}"),
        None => format!("[AllResolutions] FontName={family}"),
    };
    Some(TcLook {
        key: "font.list".to_owned(),
        source,
        kind,
    })
}

#[cfg(test)]
#[path = "total_commander_look_tests.rs"]
pub(crate) mod tests;
