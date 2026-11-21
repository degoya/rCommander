use ttf_parser::Face;

use super::{BORDER, Measures, button_style, drawn};
use crate::{app::button_bar::BarSettings, theme};

fn measures(icon_size: u16, padding: u16) -> Measures {
    Measures::of(&BarSettings {
        icon_size,
        padding,
        ..BarSettings::default()
    })
}

/// RC-110-15: the symbol's size and the room around it make the button, in both bars; the
/// defaults keep 0.21's measures (a 24-pixel symbol, 5 pixels and the frame on each side).
#[test]
fn the_size_and_the_room_make_the_buttons_of_both_bars() {
    let standard = Measures::of(&BarSettings::default());
    assert_eq!(standard, measures(24, 5));
    assert_eq!(standard.button(24.0), 24.0 + 2.0 * (5.0 + BORDER));
    assert_eq!(standard.vertical_width(), 36.0);
    for size in [16, 24, 32, 40, 48] {
        let at = measures(size, 5);
        let side = f32::from(size) + 12.0;
        assert_eq!(at.button(f32::from(size)), side, "{size}");
        assert_eq!(at.vertical_height(12.0), side, "{size}");
    }
    // The vertical bar keeps room for words beside a small symbol.
    assert_eq!(measures(16, 5).vertical_width(), 36.0);
    assert_eq!(measures(48, 5).vertical_width(), 60.0);
    // No room: the frame alone; the most room: 12 on each side.
    assert_eq!(measures(24, 0).button(24.0), 26.0);
    assert_eq!(measures(24, 12).button(24.0), 50.0);
    assert_eq!(measures(32, 12).vertical_width(), 58.0);
}

/// A flat button draws nothing until the pointer or the keyboard is on it; a framed one always
/// draws its frame, as in 0.21.
#[test]
fn a_flat_button_shows_only_under_the_pointer_and_the_keyboard() {
    for dark in [false, true] {
        let tokens = theme::tokens(dark);
        assert!(!drawn(&button_style(tokens, true, false, false)));
        assert!(drawn(&button_style(tokens, true, false, true)), "hover");
        let focused = button_style(tokens, true, true, false);
        assert!(drawn(&focused), "focus");
        assert_eq!(focused.border.color, tokens.accent);
        assert!(drawn(&button_style(tokens, false, false, false)));
        assert_eq!(
            button_style(tokens, false, false, true).border.color,
            button_style(tokens, false, false, false).border.color,
            "a framed button looks as before under the pointer"
        );
    }
}

/// The ink of every glyph of `font` as a bar button of `measures` draws it: left and right
/// edge from the button's left edge, and where its designer put it in the em square.
fn inks(font: &'static [u8], measures: Measures) -> Vec<(char, Ink)> {
    let face = Face::parse(font, 0).expect("a bundled icon font parses");
    let mut points = Vec::new();
    let cmap = face.tables().cmap.expect("the font maps characters");
    for subtable in cmap.subtables {
        if subtable.is_unicode() {
            subtable.codepoints(|point| points.push(point));
        }
    }
    points
        .into_iter()
        .filter_map(char::from_u32)
        .filter_map(|symbol| Some((symbol, ink(&face, symbol, measures)?)))
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct Ink {
    drawn: (f32, f32),
    designed: (f32, f32),
}

/// `symbol` as `view::glyph` lays it out - a text `icon` wide and high after the room on the
/// left, its advance centred in that width - and as a TrueType rasterizer (skrifa under iced's
/// text, FreeType) places its outline: the left phantom point, the outline's left edge minus
/// the bearing of `hmtx`, at the pen. A bearing that is not the outline's left edge moves the
/// drawn ink off the place the outline has in the em square.
fn ink(face: &Face, symbol: char, measures: Measures) -> Option<Ink> {
    let glyph = face.glyph_index(symbol)?;
    let outline = face.glyph_bounding_box(glyph)?;
    let scale = measures.icon / f32::from(face.units_per_em());
    let advance = f32::from(face.glyph_hor_advance(glyph)?) * scale;
    let bearing = f32::from(face.glyph_hor_side_bearing(glyph)?);
    let pen = measures.padding + (measures.icon - advance) / 2.0;
    let width = f32::from(outline.x_max - outline.x_min) * scale;
    let drawn = pen + bearing * scale;
    let designed = pen + f32::from(outline.x_min) * scale;
    Some(Ink {
        drawn: (drawn, drawn + width),
        designed: (designed, designed + width),
    })
}

/// RC-110-39: a symbol of the app's glyphs and of the collection sits in the middle of its bar
/// button, at every size and room - as its designer drew it in the em square, and on average
/// over the font on the button's centre. Before the fix the fonts' bearings were 0, and every
/// symbol was drawn moved left by its own margin, 1.4 to 4 pixels at 16 to 48.
#[test]
fn a_symbol_sits_in_the_middle_of_its_button() {
    for font in [theme::icon::FONT_BYTES, theme::symbols::FONT_BYTES] {
        for icon_size in [16, 24, 32, 40, 48] {
            for padding in [0, 5, 12] {
                let at = measures(icon_size, padding);
                let middle = (f32::from(icon_size) + 2.0 * at.padding) / 2.0;
                let inks = inks(font, at);
                assert!(!inks.is_empty());
                for (symbol, ink) in &inks {
                    let off = ink.drawn.0 - ink.designed.0;
                    assert!(
                        off.abs() < 0.01,
                        "U+{:04X} at {icon_size}/{padding} drawn {off:+.2} px off its place",
                        u32::from(*symbol)
                    );
                }
                let centre = inks
                    .iter()
                    .map(|(_, ink)| (ink.drawn.0 + ink.drawn.1) / 2.0)
                    .sum::<f32>()
                    / inks.len() as f32;
                assert!(
                    (centre - middle).abs() < 0.5,
                    "at {icon_size}/{padding} the symbols' centre is {:+.2} px off the button's",
                    centre - middle
                );
            }
        }
    }
}
