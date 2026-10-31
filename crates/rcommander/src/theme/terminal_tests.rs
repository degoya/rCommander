use super::{CAMPBELL, ONE_HALF_LIGHT, terminal_palette};
use crate::theme::{dark, light, system::DEFAULT_ACCENT};

/// Fluent keeps the terminal colours it had before themes: Campbell on dark, One Half Light on
/// light; the dimmed colours follow from the normal ones.
#[test]
fn fluent_keeps_campbell_and_one_half_light() {
    let dark = terminal_palette(&dark::tokens(DEFAULT_ACCENT.dark));
    assert_eq!(dark.foreground, "#cccccc");
    assert_eq!(dark.background, "#1c1c1c");
    assert_eq!(dark.bright_blue, "#3b78ff");
    assert_eq!(dark.dim_red, dark.red);
    assert_eq!(dark.dim_foreground, dark.bright_black);
    let light = terminal_palette(&light::tokens(DEFAULT_ACCENT.light));
    assert_eq!(light.foreground, "#383a42");
    assert_eq!(light.background, "#ffffff");
    assert_eq!(light.white, "#fafafa");
    assert_ne!(CAMPBELL, ONE_HALF_LIGHT);
}
