use super::{invisible, turns_direction};

#[test]
fn the_direction_characters_turn_the_reading_direction() {
    for c in [
        '\u{61c}', '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}',
        '\u{202e}', '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}',
    ] {
        assert!(turns_direction(c), "{c:?}");
        assert!(invisible(c), "{c:?}");
    }
    for c in ['a', ' ', '\u{200b}', '\u{feff}', '\u{e4}', '\u{5d0}'] {
        assert!(!turns_direction(c), "{c:?}");
    }
}

#[test]
fn zero_width_and_control_characters_are_invisible() {
    for c in [
        '\0', '\t', '\n', '\r', '\u{1b}', '\u{7f}', '\u{85}', '\u{200b}', '\u{200c}', '\u{200d}',
        '\u{feff}',
    ] {
        assert!(invisible(c), "{c:?}");
    }
    for c in ['a', ' ', '.', '\u{e4}', '\u{1f600}', '\u{5d0}', '\u{3000}'] {
        assert!(!invisible(c), "{c:?}");
    }
}
