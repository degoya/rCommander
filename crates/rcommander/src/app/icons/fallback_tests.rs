use super::{Glyph, SIDE, draw};

fn alpha_at(pixels: &[u8], x: u32, y: u32) -> u8 {
    pixels[((y * SIDE + x) * 4 + 3) as usize]
}

#[test]
fn the_generic_icons_are_square_with_a_transparent_border() {
    for glyph in [Glyph::File, Glyph::Folder] {
        for dark in [false, true] {
            let icon = draw(glyph, dark);
            assert_eq!((icon.width, icon.height), (SIDE, SIDE));
            assert_eq!(icon.pixels.len(), (SIDE * SIDE * 4) as usize);
            assert_eq!(alpha_at(&icon.pixels, 0, 0), 0, "{glyph:?}: corner");
            assert_eq!(alpha_at(&icon.pixels, SIDE - 1, SIDE - 1), 0, "{glyph:?}");
            assert_eq!(alpha_at(&icon.pixels, SIDE / 2, SIDE / 2), 255, "{glyph:?}");
        }
    }
}

#[test]
fn edges_are_smooth_not_black() {
    let icon = draw(Glyph::File, false);
    let partial = icon
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|pixel| pixel[3] != 0 && pixel[3] != 255);
    let mut seen = 0;
    for pixel in partial {
        seen += 1;
        // A soft edge keeps the shape's colour; only its alpha fades.
        assert!(pixel[0] > 0x40, "{pixel:?}");
    }
    assert!(seen > 0, "the folded corner has partly covered pixels");
}

#[test]
fn file_and_folder_differ_and_each_look_has_its_own() {
    assert_ne!(draw(Glyph::File, false), draw(Glyph::Folder, false));
    assert_ne!(draw(Glyph::File, false), draw(Glyph::File, true));
    assert_ne!(draw(Glyph::Folder, false), draw(Glyph::Folder, true));
}
