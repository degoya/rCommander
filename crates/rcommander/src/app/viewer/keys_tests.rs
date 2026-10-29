use rc_engine::{
    Chord,
    viewer::{TextEncoding, ViewMode},
};

use super::{ListerKey, Scroll, action};

fn key(text: &str) -> Option<ListerKey> {
    action(&text.parse::<Chord>().expect("a chord"))
}

#[test]
fn listers_mode_keys_switch_the_view() {
    assert_eq!(key("1"), Some(ListerKey::Mode(ViewMode::Text)));
    assert_eq!(key("2"), Some(ListerKey::Mode(ViewMode::Binary)));
    assert_eq!(key("3"), Some(ListerKey::Mode(ViewMode::Hex)));
    assert_eq!(key("4"), Some(ListerKey::Mode(ViewMode::Image)));
    assert_eq!(key("5"), Some(ListerKey::Preview));
    assert_eq!(key("Ctrl+5"), None);
    // RC-100-10: a plugin document's views, beside the modes.
    assert_eq!(key("7"), Some(ListerKey::NextView));
    assert_eq!(key("Ctrl+7"), None);
}

#[test]
fn encodings_have_listers_keys_and_one_to_cycle() {
    assert_eq!(
        key("A"),
        Some(ListerKey::Encoding(TextEncoding::Windows1252))
    );
    assert_eq!(key("S"), Some(ListerKey::Encoding(TextEncoding::Cp437)));
    assert_eq!(key("Ctrl+S"), None);
    assert_eq!(key("6"), Some(ListerKey::Encoding(TextEncoding::Utf16Le)));
    assert_eq!(key("8"), Some(ListerKey::Encoding(TextEncoding::Utf8)));
    assert_eq!(key("E"), Some(ListerKey::NextEncoding));
}

#[test]
fn f3_searches_on_and_shift_f3_back_f7_and_ctrl_f_open_the_search() {
    assert_eq!(key("F3"), Some(ListerKey::FindNext));
    assert_eq!(key("Shift+F3"), Some(ListerKey::FindPrevious));
    assert_eq!(key("F7"), Some(ListerKey::Search));
    assert_eq!(key("Ctrl+F"), Some(ListerKey::Search));
}

#[test]
fn escape_closes_and_n_p_step_through_the_panel() {
    assert_eq!(key("Escape"), Some(ListerKey::Close));
    assert_eq!(key("N"), Some(ListerKey::NextFile));
    assert_eq!(key("P"), Some(ListerKey::PreviousFile));
    assert_eq!(key("F2"), Some(ListerKey::Reload));
}

#[test]
fn scrolling_and_image_keys() {
    assert_eq!(key("PageDown"), Some(ListerKey::Scroll(Scroll::PageDown)));
    assert_eq!(key("Space"), Some(ListerKey::Scroll(Scroll::PageDown)));
    assert_eq!(key("Ctrl+End"), Some(ListerKey::Scroll(Scroll::End)));
    assert_eq!(key("NumAdd"), Some(ListerKey::ZoomIn));
    assert_eq!(key("-"), Some(ListerKey::ZoomOut));
    assert_eq!(key("F"), Some(ListerKey::Fit));
    assert_eq!(key("R"), Some(ListerKey::RotateRight));
    assert_eq!(key("Shift+R"), Some(ListerKey::RotateLeft));
}

/// RC-020-12: Ctrl with `+`, `=`, NumAdd, `-`, NumSubtract and 0 sizes the text; on a German
/// layout `+` is a key of its own (`Plus`), on a US one Shift+=.
#[test]
fn ctrl_plus_minus_and_zero_size_the_text() {
    for larger in ["Ctrl+Plus", "Ctrl+=", "Ctrl+Shift+=", "Ctrl+NumAdd"] {
        assert_eq!(key(larger), Some(ListerKey::TextLarger), "{larger}");
    }
    for smaller in ["Ctrl+-", "Ctrl+NumSubtract"] {
        assert_eq!(key(smaller), Some(ListerKey::TextSmaller), "{smaller}");
    }
    assert_eq!(key("Ctrl+0"), Some(ListerKey::TextReset));
    assert!(ListerKey::TextReset.sizes_text());
    assert!(!ListerKey::ZoomIn.sizes_text());
    // Without Ctrl the keys stay the image's, `+` of the main block among them.
    assert_eq!(key("Plus"), Some(ListerKey::ZoomIn));
    assert_eq!(key("="), Some(ListerKey::ZoomIn));
    assert_eq!(key("0"), None);
    assert_eq!(key("Ctrl+Alt+Plus"), None);
    assert_eq!(key("Ctrl+Alt+0"), None);
}

#[test]
fn other_chords_do_nothing() {
    assert_eq!(key("Ctrl+1"), None);
    assert_eq!(key("Alt+N"), None);
    assert_eq!(key("Tab"), None);
    assert_eq!(key("Ctrl+C"), None);
}
