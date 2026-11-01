use super::*;

/// A change is told once; each mode keeps its own theme, and a theme the window does not know
/// has no look - the chosen one draws.
#[test]
fn the_themes_are_kept_per_mode() {
    let known: &'static Tokens = crate::theme::fixed(true);
    let lookup = move |id: &str, dark: bool| (dark && id == "rc68-test-dark").then_some(known);
    let themes = [
        Some("rc68-unknown".to_owned()),
        Some("rc68-test-dark".to_owned()),
    ];
    assert!(set_themes(themes.clone()));
    assert!(!set_themes(themes), "the same again changes nothing");
    resolve(lookup);
    assert!(look(true).is_some_and(|tokens| std::ptr::eq(tokens, known)));
    assert!(look(false).is_none(), "unknown: the chosen theme draws");
    assert!(set_themes([None, None]));
    resolve(lookup);
    assert!(look(true).is_none());
}
