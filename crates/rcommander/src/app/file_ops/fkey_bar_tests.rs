use rc_engine::Keymap;

use super::*;

/// The bar's word for each key, plain and with Shift, from the standard keymap.
#[test]
fn the_bar_names_what_each_key_does_with_its_modifiers() {
    let keymap = Keymap::standard();
    let label = |keys: &str| {
        let chord: Chord = keys.parse().expect("a chord");
        keymap.lookup(&chord, PanelView::Files).map_or("", label_of)
    };
    assert_eq!(label("F3"), t!("fkey.view"));
    assert_eq!(label("F4"), t!("fkey.edit"));
    assert_eq!(label("F5"), t!("fkey.copy"));
    assert_eq!(label("F6"), t!("fkey.move"));
    assert_eq!(label("F7"), t!("fkey.mkdir"));
    assert_eq!(label("F8"), t!("fkey.delete"));
    assert_eq!(label("Shift+F4"), t!("fkey.new_text"));
    assert_eq!(label("Shift+F5"), t!("fkey.copy_here"));
    assert_eq!(label("Shift+F6"), t!("fkey.rename"));
    assert_eq!(label("Shift+F8"), t!("fkey.delete_permanent"));
    assert_eq!(label("Ctrl+F3"), t!("fkey.sort_name"));
    assert_eq!(
        label("Shift+F7"),
        "",
        "a key without a command shows no word"
    );
}
