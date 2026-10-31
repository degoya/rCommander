use std::path::Path;

use rc_engine::settings::themes::{BUILTIN_THEMES, Theme, ThemeAccent, parse_file};

use super::{exported, own_colours, path_of, read, write};

#[test]
fn the_field_names_an_absolute_file() {
    let root = if cfg!(windows) {
        "C:\\themes"
    } else {
        "/themes"
    };
    assert!(path_of("", false).is_none());
    assert!(path_of("night.rctheme", false).is_none(), "relative");
    assert!(path_of(&format!("{root}/a\u{7}.rctheme"), false).is_none());
    let export = path_of(&format!("{root}/night"), true).expect("a file");
    assert!(export.to_string_lossy().ends_with("night.rctheme"));
    let kept = path_of(&format!("{root}/night.RCTHEME"), true).expect("a file");
    assert!(kept.to_string_lossy().ends_with("night.RCTHEME"));
}

/// A built-in theme goes out with its colours as they are seen and reads back as an own theme;
/// Fluent leaves the colours made of the system's accent to it.
#[test]
fn a_builtin_is_written_with_its_colours() {
    let nord = BUILTIN_THEMES
        .iter()
        .find(|theme| theme.id == "nord-dark")
        .map(Theme::of_builtin)
        .expect("nord");
    let read = parse_file(&exported(&nord)).expect("reads");
    assert!(read.dark);
    assert_eq!(read.accent, ThemeAccent::Colour("#88c0d0".into()));
    assert_eq!(
        read.tokens.get("terminal.background").map(String::as_str),
        Some("#2e3440")
    );
    let fluent = BUILTIN_THEMES
        .iter()
        .find(|theme| theme.id == "fluent-light")
        .map(Theme::of_builtin)
        .expect("fluent");
    let (accent, tokens) = own_colours(&fluent);
    assert_eq!(accent, ThemeAccent::System);
    assert!(!tokens.contains_key("cursor_fill"));
    assert!(tokens.contains_key("window"));
}

/// The import: a file written reads back; one too large is refused before it is parsed.
#[tokio::test]
async fn a_file_is_written_and_read_back_and_a_large_one_refused() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("night.rctheme");
    let nord = BUILTIN_THEMES
        .iter()
        .find(|theme| theme.id == "nord-light")
        .map(Theme::of_builtin)
        .expect("nord");
    write(path.clone(), exported(&nord)).await.expect("written");
    let theme = read(path).await.expect("read");
    assert!(!theme.dark);
    let large = dir.path().join("large.rctheme");
    std::fs::write(&large, vec![b' '; 64 * 1024 + 10]).expect("large");
    assert_eq!(read(large).await, Err("theme_file_too_large"));
    assert_eq!(
        read(Path::new("/no/such/file.rctheme").to_path_buf()).await,
        Err("theme_file_unreadable")
    );
}
