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

/// RC-110-62: every other format character (Unicode's category Cf) is invisible too - the soft
/// hyphen, the word joiner, the tag characters that hide text in a name.
#[test]
fn the_other_format_characters_are_invisible() {
    for c in [
        '\u{ad}',
        '\u{600}',
        '\u{6dd}',
        '\u{70f}',
        '\u{180e}',
        '\u{2060}',
        '\u{2064}',
        '\u{206a}',
        '\u{206f}',
        '\u{fff9}',
        '\u{110bd}',
        '\u{13430}',
        '\u{1bca0}',
        '\u{1d173}',
        '\u{e0001}',
        '\u{e0041}',
        '\u{e007f}',
    ] {
        assert!(invisible(c), "{c:?}");
        assert!(!turns_direction(c), "{c:?}");
    }
    for c in ['\u{2065}', '\u{a0}', '\u{202f}', '\u{fe0f}', '\u{e0100}'] {
        assert!(!invisible(c), "{c:?}");
    }
}
