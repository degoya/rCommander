use rc_engine::settings::themes::THEME_TOKENS;

use super::{GROUPS, token_label};

/// Every token stands in exactly one group, in the engine's order, and has its own label.
#[test]
fn every_token_is_in_one_group_with_a_label() {
    let grouped: Vec<&str> = GROUPS
        .iter()
        .flat_map(|(_, keys)| keys.iter().copied())
        .collect();
    assert_eq!(grouped, THEME_TOKENS.as_slice());
    for key in THEME_TOKENS {
        assert_ne!(token_label(key), key, "{key} has no label");
    }
}
