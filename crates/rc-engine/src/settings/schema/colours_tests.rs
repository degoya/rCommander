use serde_json::Value;

use super::{
    COLOUR_KEYS, COLOUR_RULES_KEY, ColourRule, MAX_COLOUR_RULES, colour_key, colour_value,
    parse_colour_rules, write_colour_rules,
};
use crate::settings::schema::setting;

#[test]
fn a_colour_is_kept_as_lower_case_rrggbb_or_empty() {
    assert_eq!(colour_value("#C42B1C").as_deref(), Some("#c42b1c"));
    assert_eq!(colour_value(" #abc ").as_deref(), Some("#aabbcc"));
    assert_eq!(colour_value("").as_deref(), Some(""));
    for wrong in ["c42b1c", "#c42b1", "#c42b1cff", "#ggg", "red"] {
        assert_eq!(colour_value(wrong), None, "{wrong}");
    }
}

#[test]
fn every_colour_key_is_a_schema_row_and_found_by_its_token() {
    for key in COLOUR_KEYS {
        assert!(setting(key).is_some(), "{key}");
    }
    assert_eq!(
        colour_key(true, "cursor_text"),
        Some("appearance.colours.dark.cursor_text")
    );
    assert_eq!(colour_key(false, "nothing"), None);
}

#[test]
fn a_colour_setting_checks_its_value() {
    let spec = setting("appearance.colours.light.text").expect("a row");
    assert_eq!(spec.check(&Value::from("#FFF")), Ok(Value::from("#ffffff")));
    assert!(spec.check(&Value::from("white")).is_err());
    assert_eq!(spec.default_value(), Value::from(""));
}

#[test]
fn rules_read_in_their_order_and_write_back_the_same() {
    let text = "*.exe;*.bat\t#C42B1C\t#FF99A4\n\n*\\\t#0078d4";
    let rules = parse_colour_rules(text).expect("rules");
    assert_eq!(
        rules,
        [
            ColourRule {
                mask: "*.exe;*.bat".to_owned(),
                light: "#c42b1c".to_owned(),
                dark: Some("#ff99a4".to_owned()),
            },
            ColourRule {
                mask: "*\\".to_owned(),
                light: "#0078d4".to_owned(),
                dark: None,
            },
        ]
    );
    let written = write_colour_rules(&rules);
    assert_eq!(parse_colour_rules(&written), Some(rules));
    let spec = setting(COLOUR_RULES_KEY).expect("a row");
    assert_eq!(spec.check(&Value::from(text)), Ok(Value::from(written)));
}

#[test]
fn a_broken_rule_list_is_refused() {
    for wrong in [
        "\t#c42b1c",
        "*.exe",
        "*.exe\tred",
        "*.exe\t#c42b1c\t#zzzzzz",
        "*.exe\t#c42b1c\t#000000\textra",
    ] {
        assert_eq!(parse_colour_rules(wrong), None, "{wrong:?}");
    }
    let many = vec!["*.x\t#000000"; MAX_COLOUR_RULES + 1].join("\n");
    assert_eq!(parse_colour_rules(&many), None);
    assert_eq!(parse_colour_rules(""), Some(Vec::new()));
}
