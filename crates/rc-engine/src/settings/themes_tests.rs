use rc_db::StoredTheme;

use super::{
    BUILTIN_THEMES, THEME_TOKENS, Theme, ThemeAccent, builtin, clean_name, clean_tokens,
    hex_colour, is_own_id, new_own_id, unique_name,
};

#[test]
fn every_builtin_has_both_modes_and_a_unique_id() {
    let mut ids: Vec<&str> = BUILTIN_THEMES.iter().map(|theme| theme.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), BUILTIN_THEMES.len());
    for theme in BUILTIN_THEMES {
        let twin = BUILTIN_THEMES
            .iter()
            .any(|other| other.name == theme.name && other.dark != theme.dark);
        assert!(twin, "{} has no theme of the other mode", theme.id);
        assert!(!is_own_id(theme.id));
    }
    assert!(builtin("fluent-dark").is_some());
    assert_eq!(builtin("mac-light").is_some(), cfg!(target_os = "macos"));
}

#[test]
fn the_tokens_are_unique() {
    let mut tokens = THEME_TOKENS.to_vec();
    tokens.sort_unstable();
    tokens.dedup();
    assert_eq!(tokens.len(), THEME_TOKENS.len());
}

#[test]
fn colours_names_and_ids_follow_their_rules() {
    assert_eq!(hex_colour("#ABCDEF").as_deref(), Some("#abcdef"));
    for wrong in ["", "abcdef", "#abc", "#abcdeg", "#abcdef0"] {
        assert_eq!(hex_colour(wrong), None, "{wrong}");
    }
    assert_eq!(clean_name("  Night\t\u{0}  "), "Night");
    assert_eq!(clean_name(&"a".repeat(80)).chars().count(), 64);
    assert!(is_own_id(&new_own_id()));
    assert!(!is_own_id("own-"));
    assert!(!is_own_id("own-UPPER"));
    assert_eq!(ThemeAccent::parse("System"), Some(ThemeAccent::System));
    assert_eq!(
        ThemeAccent::parse("#FF8000"),
        Some(ThemeAccent::Colour("#ff8000".into()))
    );
    assert_eq!(ThemeAccent::parse("orange"), None);
}

#[test]
fn a_name_taken_gets_a_number() {
    let taken = ["Night", "night (2)"];
    assert_eq!(unique_name("Day", taken.iter().copied()), "Day");
    assert_eq!(unique_name("NIGHT", taken.iter().copied()), "NIGHT (3)");
    let long = "x".repeat(64);
    let named = unique_name(&long, [long.as_str()].into_iter());
    assert_eq!(named.chars().count(), 64);
    assert!(named.ends_with(" (2)"));
}

#[test]
fn tokens_are_kept_known_and_checked() {
    let (kept, unknown) = clean_tokens([("window", "#FFFFFF"), ("glow", "#000000")]).expect("ok");
    assert_eq!(kept.get("window").map(String::as_str), Some("#ffffff"));
    assert_eq!(unknown, ["glow"]);
    assert_eq!(
        clean_tokens([("window", "white")]),
        Err("window".to_owned())
    );
}

/// A row read back is checked: a token not known or not a colour is dropped, a row without an
/// own id is left out.
#[test]
fn a_stored_row_is_checked_on_the_way_back() {
    let row = |id: &str, tokens: &str| StoredTheme {
        id: id.to_owned(),
        name: "Night".to_owned(),
        dark: true,
        accent: "#00ff00".to_owned(),
        tokens: tokens.to_owned(),
        saved_at: String::new(),
    };
    let theme = Theme::from_stored(row(
        "own-1",
        r##"{"window":"#101010","glow":"#000000","surface":"red"}"##,
    ))
    .expect("an own theme");
    assert_eq!(theme.tokens.len(), 1);
    assert_eq!(theme.accent, ThemeAccent::Colour("#00ff00".into()));
    assert!(!theme.builtin);
    assert!(Theme::from_stored(row("fluent-dark", "{}")).is_none());
    let back = Theme::from_stored(theme.to_stored()).expect("again");
    assert_eq!(back, theme);
}
