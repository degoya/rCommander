//! The editor's logic without a window, on tokio's runtime, which the pause's timer needs. Its
//! themes are of the light mode: a preview it draws never stands for the dark mode the theme tests
//! preview (`theme/themes_tests.rs`).

use std::collections::BTreeMap;

use rc_engine::settings::themes::{Theme, ThemeAccent};

use super::{Editor, EditorMsg, Slot};
use crate::theme;

fn day() -> Theme {
    Theme {
        id: "own-editor".into(),
        name: "Day".into(),
        dark: false,
        accent: ThemeAccent::System,
        tokens: BTreeMap::from([("header".to_owned(), "#eeeeee".to_owned())]),
        builtin: false,
    }
}

/// Each change asks for one pause; only the pause after the last change builds the look, so a
/// look is made per pause and never per key.
#[tokio::test]
async fn a_look_is_built_after_the_last_change_only() {
    let mut editor = Editor::open(day());
    let before = editor.shown;
    for text in ["#1", "#10", "#102", "#1020", "#10203", "#102030"] {
        let (effects, close) = editor.update(EditorMsg::Hex(Slot::Token("window"), text.into()));
        assert_eq!(effects.len(), 1, "one pause per change");
        assert!(!close);
    }
    let _ = editor.update(EditorMsg::Pause(3));
    assert!(
        std::ptr::eq(editor.shown, before),
        "a pause of an earlier change"
    );
    let _ = editor.update(EditorMsg::Pause(6));
    assert!(
        !std::ptr::eq(editor.shown, before),
        "the last change's pause"
    );
    assert_eq!(
        editor.shown.window,
        iced::Color::from_rgb8(0x10, 0x20, 0x30)
    );
}

/// Reset goes back to the theme as it opened: its own colour, or none for a token it lacked.
#[tokio::test]
async fn reset_goes_back_to_the_theme_as_it_opened() {
    let mut editor = Editor::open(day());
    let _ = editor.update(EditorMsg::Hex(Slot::Token("header"), "#000000".into()));
    let _ = editor.update(EditorMsg::Hex(Slot::Token("stroke"), "#ff0000".into()));
    assert!(editor.changed("header") && editor.changed("stroke"));
    let _ = editor.update(EditorMsg::Reset("header"));
    let _ = editor.update(EditorMsg::Reset("stroke"));
    assert_eq!(
        editor.theme.tokens.get("header").map(String::as_str),
        Some("#eeeeee")
    );
    assert!(!editor.theme.tokens.contains_key("stroke"));
    assert!(!editor.changed("header") && !editor.changed("stroke"));
}

/// The accent: an own colour starts as the one drawn and follows what is typed; the system's
/// again takes it away. A colour emptied falls back to the fixed set's.
#[tokio::test]
async fn the_accent_and_an_emptied_colour() {
    let mut editor = Editor::open(day());
    let _ = editor.update(EditorMsg::SystemAccent(false));
    let drawn = theme::themes::hex_of(editor.shown.accent);
    assert_eq!(editor.theme.accent, ThemeAccent::Colour(drawn));
    let _ = editor.update(EditorMsg::Hex(Slot::Accent, "#FF8000".into()));
    assert_eq!(editor.theme.accent, ThemeAccent::Colour("#ff8000".into()));
    let _ = editor.update(EditorMsg::SystemAccent(true));
    assert_eq!(editor.theme.accent, ThemeAccent::System);
    let _ = editor.update(EditorMsg::Hex(Slot::Token("header"), String::new()));
    assert!(!editor.theme.tokens.contains_key("header"));
    let (_, close) = editor.update(EditorMsg::Cancel);
    assert!(close);
}

/// The name is saved as the name rule keeps it.
#[tokio::test]
async fn the_saved_name_follows_the_rule() {
    let mut editor = Editor::open(day());
    let _ = editor.update(EditorMsg::Name("  Dawn\u{7}  ".into()));
    assert_eq!(editor.saved().name, "Dawn");
}
