use std::collections::BTreeSet;

use ttf_parser::{Face, name_id};

use super::{FAMILY, FONT_BYTES, Icon};

fn face() -> Face<'static> {
    Face::parse(FONT_BYTES, 0).expect("the bundled icon font parses")
}

#[test]
fn every_icon_has_its_own_glyph_in_the_font() {
    let face = face();
    let glyphs: BTreeSet<u16> = Icon::ALL
        .iter()
        .map(|icon| {
            face.glyph_index(icon.glyph())
                .unwrap_or_else(|| panic!("{icon:?} has no glyph; run scripts/icon-font.sh"))
                .0
        })
        .collect();
    assert_eq!(glyphs.len(), Icon::ALL.len(), "two icons share a glyph");
}

#[test]
fn the_font_holds_only_the_icons_glyphs() {
    let face = face();
    let mut mapped = BTreeSet::new();
    let cmap = face.tables().cmap.expect("the font maps characters");
    for subtable in cmap.subtables {
        if subtable.is_unicode() {
            subtable.codepoints(|point| {
                mapped.insert(point);
            });
        }
    }
    let icons: BTreeSet<u32> = Icon::ALL
        .iter()
        .map(|icon| u32::from(icon.glyph()))
        .collect();
    assert_eq!(
        mapped, icons,
        "the glyphs of scripts/icon-font.sh and Icon differ"
    );
    // The icons and .notdef: nothing of the full font is left in it.
    assert_eq!(usize::from(face.number_of_glyphs()), Icon::ALL.len() + 1);
}

#[test]
fn the_font_is_named_so_no_installed_font_stands_in_for_it() {
    let face = face();
    let family = face
        .names()
        .into_iter()
        .find(|name| name.name_id == name_id::FAMILY)
        .and_then(|name| name.to_string());
    assert_eq!(family.as_deref(), Some(FAMILY));
}

#[test]
fn the_icons_stay_small() {
    // The job's budget is 300 KB of binary growth; the font is most of it.
    assert!(FONT_BYTES.len() < 16 * 1024, "{} bytes", FONT_BYTES.len());
}
