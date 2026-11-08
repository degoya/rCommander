use std::collections::BTreeSet;

use rc_engine::user_commands::symbols;
use ttf_parser::{Face, name_id};

use super::{FAMILY, FONT_BYTES, glyph_of};

fn face() -> Face<'static> {
    Face::parse(FONT_BYTES, 0).expect("the bundled symbol font parses")
}

#[test]
fn the_font_holds_exactly_the_tables_symbols() {
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
    let table: BTreeSet<u32> = symbols::all()
        .iter()
        .map(|symbol| u32::from(symbol.glyph))
        .collect();
    assert_eq!(
        mapped, table,
        "the font and symbols.tsv differ; run scripts/icon-font.sh --symbols"
    );
    // The symbols and .notdef: nothing of the full font is left in it.
    assert_eq!(
        usize::from(face.number_of_glyphs()),
        symbols::all().len() + 1
    );
    for symbol in symbols::all() {
        assert!(
            face.glyph_index(symbol.glyph).is_some(),
            "{} has no glyph",
            symbol.name
        );
    }
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
fn the_collection_stays_in_its_budget() {
    // The job's budget: at most 400 KiB.
    assert!(FONT_BYTES.len() <= 400 * 1024, "{} bytes", FONT_BYTES.len());
}

#[test]
fn a_reference_draws_its_glyph_and_anything_else_none() {
    let folder = symbols::find("folder").expect("listed");
    assert_eq!(glyph_of("rc-symbol:folder"), Some(folder.glyph));
    assert_eq!(glyph_of("rc-symbol:no_such_symbol"), None);
    assert_eq!(glyph_of("C:\\Tools\\WCMICONS.DLL"), None);
    assert_eq!(glyph_of(""), None);
}
