use iced::Font;

use super::{AreaFont, Family, Fonts, families, intern_in, resolve, sizes_of};
use crate::theme::{ROW_HEIGHT, TEXT_SIZE};

fn list_at(size: f32) -> AreaFont {
    AreaFont {
        size,
        ..Fonts::DEFAULT.list
    }
}

#[test]
fn the_row_height_follows_the_font_size() {
    assert_eq!(list_at(TEXT_SIZE).row_height(), ROW_HEIGHT, "as before");
    assert_eq!(list_at(8.0).row_height(), 17.0);
    assert_eq!(list_at(24.0).row_height(), 38.0);
    let heights: Vec<f32> = (8..=24)
        .map(|size| list_at(size as f32).row_height())
        .collect();
    assert!(
        heights.windows(2).all(|pair| pair[0] <= pair[1]),
        "{heights:?}"
    );
    assert!(
        (8..=24).all(|size| list_at(size as f32).row_height() > size as f32 * 1.2),
        "a row holds its line"
    );
    assert_eq!(Fonts::DEFAULT.viewer.line_height(), 18.0, "as before");
}

#[test]
fn the_fixed_columns_widen_with_the_size_and_in_a_monospaced_font() {
    assert_eq!(Fonts::DEFAULT.list.column_scale(), 1.0);
    assert_eq!(list_at(26.0).column_scale(), 2.0);
    let mono = AreaFont {
        mono: true,
        ..Fonts::DEFAULT.list
    };
    assert!(mono.column_scale() > 1.0);
    assert!(mono.width("illl") > Fonts::DEFAULT.list.width("illl"));
    assert_eq!(mono.shorten("short.txt", 1000.0), "short.txt");
}

#[test]
fn an_unknown_font_falls_back_to_the_default_in_the_chosen_size() {
    let default = Fonts::DEFAULT.list;
    let none = |_: &str| None;
    let resolved = resolve("No Such Font", 16, 8..=24, default, none);
    assert_eq!(resolved.font, default.font);
    assert_eq!(resolved.size, 16.0);
    assert!(!resolved.mono);
    assert_eq!(resolve("", 13, 8..=24, default, |_| Some(true)), default);
}

#[test]
fn an_installed_font_is_taken_with_its_kind() {
    let installed = |name: &str| (name == "Cascadia Mono").then_some(true);
    let resolved = resolve("Cascadia Mono", 15, 8..=24, Fonts::DEFAULT.list, installed);
    assert_eq!(resolved.font, Font::with_name("Cascadia Mono"));
    assert_eq!(resolved.size, 15.0);
    assert!(resolved.mono);
}

#[test]
fn a_size_outside_the_schema_is_held_to_it() {
    let default = Fonts::DEFAULT.terminal;
    let sizes = sizes_of(rc_engine::settings::TERMINAL_FONT_SIZE_KEY);
    assert_eq!(resolve("", 2, sizes.clone(), default, |_| None).size, 8.0);
    assert_eq!(resolve("", 99, sizes, default, |_| None).size, 24.0);
    // The viewer's reaches further: its text size keys go from 6 to 72 (RC-020-12).
    let viewer = sizes_of(rc_engine::settings::VIEWER_FONT_SIZE_KEY);
    assert_eq!(viewer, 6..=72);
    let default = Fonts::DEFAULT.viewer;
    assert_eq!(
        resolve("", 40, viewer.clone(), default, |_| None).size,
        40.0
    );
    assert_eq!(resolve("", 99, viewer, default, |_| None).size, 72.0);
}

#[test]
fn names_are_kept_once_and_only_up_to_the_limit() {
    let mut names = Vec::new();
    let first = intern_in(&mut names, "Consolas", 2).expect("room");
    let again = intern_in(&mut names, "Consolas", 2).expect("kept");
    assert!(std::ptr::eq(first, again), "the same name is kept once");
    assert!(intern_in(&mut names, "Cascadia Mono", 2).is_some());
    assert_eq!(intern_in(&mut names, "Courier New", 2), None, "full");
    assert_eq!(intern_in(&mut names, "Consolas", 2), Some("Consolas"));
    assert_eq!(names.len(), 2);
}

#[test]
fn the_installed_families_are_sorted_once_each_without_hidden_ones() {
    let faces = [
        ("consolas", true),
        ("Segoe UI", false),
        ("Cascadia Mono", false),
        ("Cascadia Mono", true),
        ("Segoe UI", false),
        (".Hidden", false),
        ("Iced-Icons", false),
        ("rCommander Icons", false),
        ("", true),
    ]
    .map(|(name, mono)| (name.to_owned(), mono));
    let family = |name: &str, mono| Family {
        name: name.to_owned(),
        mono,
    };
    assert_eq!(
        families(faces.into_iter()),
        [
            family("Cascadia Mono", true),
            family("consolas", true),
            family("Segoe UI", false),
        ]
    );
}

#[test]
fn a_block_height_is_kept_only_when_a_line_could_be_that_tall() {
    assert_eq!(super::plausible_block(1.16), Some(1.16));
    assert_eq!(super::plausible_block(0.0), None, "an empty glyph");
    assert_eq!(super::plausible_block(5.0), None, "a broken font");
    assert_eq!(
        Fonts::DEFAULT.dos_block,
        None,
        "measured only once the settings are read"
    );
}
